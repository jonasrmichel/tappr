use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rodio::Source;

use crate::audio::{LoopBuffer, CHANNELS, SAMPLE_RATE};

/// Shared state for controlling the looping source
pub struct LoopControl {
    /// Current buffer being played
    current: LoopBuffer,
    /// Pending buffer to swap to at bar boundary
    pending: Option<LoopBuffer>,
    /// Current playback position (sample index)
    position: usize,
}

impl LoopControl {
    fn new(buffer: LoopBuffer) -> Self {
        Self {
            current: buffer,
            pending: None,
            position: 0,
        }
    }

    /// Queue a new buffer to swap at the next bar boundary
    pub fn queue_swap(&mut self, buffer: LoopBuffer) {
        self.pending = Some(buffer);
    }

    /// Check if at a bar boundary (within a small tolerance)
    fn at_bar_boundary(&self) -> bool {
        let samples_per_bar = self.current.samples_per_bar();
        if samples_per_bar == 0 {
            return false;
        }
        let position_in_bar = self.position % samples_per_bar;
        // Allow swap within first 64 samples of a bar
        position_in_bar < 64
    }

    /// Get the next sample and advance position
    fn next_sample(&mut self) -> f32 {
        // Check for pending swap at bar boundary
        if self.pending.is_some() && self.at_bar_boundary() {
            if let Some(new_buffer) = self.pending.take() {
                self.current = new_buffer;
                self.position = 0;
            }
        }

        let sample = self.current.samples[self.position];
        self.position = (self.position + 1) % self.current.samples.len();
        sample
    }
}

/// Custom rodio source for seamless looping with hot-swap capability
pub struct LoopingSource {
    control: Arc<Mutex<LoopControl>>,
}

impl LoopingSource {
    /// Create a new looping source with the given buffer
    pub fn new(buffer: LoopBuffer) -> (Self, Arc<Mutex<LoopControl>>) {
        let control = Arc::new(Mutex::new(LoopControl::new(buffer)));
        let source = Self {
            control: Arc::clone(&control),
        };
        (source, control)
    }
}

impl Source for LoopingSource {
    fn current_frame_len(&self) -> Option<usize> {
        // Return None for infinite source
        None
    }

    fn channels(&self) -> u16 {
        CHANNELS
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        // Infinite looping
        None
    }
}

impl Iterator for LoopingSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let mut control = self.control.lock();
        Some(control.next_sample())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LoopInfo;

    fn create_test_buffer(samples: Vec<f32>, bars: u8) -> LoopBuffer {
        let duration_samples = samples.len() / CHANNELS as usize;
        LoopBuffer::new(
            samples,
            LoopInfo {
                bpm: 120.0,
                bpm_confidence: 1.0,
                bars,
                beats_per_bar: 4,
                duration_samples,
                sample_rate: SAMPLE_RATE,
            },
        )
    }

    #[test]
    fn test_looping_source_loops() {
        // Create a small buffer
        let samples = vec![0.1, 0.2, 0.3, 0.4]; // 2 stereo frames
        let buffer = create_test_buffer(samples.clone(), 1);

        let (mut source, _control) = LoopingSource::new(buffer);

        // Read more samples than the buffer contains
        let output: Vec<f32> = (0..8).filter_map(|_| source.next()).collect();

        // Should loop: [0.1, 0.2, 0.3, 0.4, 0.1, 0.2, 0.3, 0.4]
        assert_eq!(output.len(), 8);
        assert_eq!(output[0], 0.1);
        assert_eq!(output[4], 0.1); // Looped
    }

    #[test]
    fn test_buffer_swap() {
        // Create initial buffer
        let samples1 = vec![0.1; 128]; // Small buffer
        let buffer1 = create_test_buffer(samples1, 1);

        let (mut source, control) = LoopingSource::new(buffer1);

        // Queue a new buffer
        let samples2 = vec![0.9; 128];
        let buffer2 = create_test_buffer(samples2, 1);

        {
            let mut ctrl = control.lock();
            ctrl.queue_swap(buffer2);
        }

        // Read until swap happens (at bar boundary)
        let mut found_new_value = false;
        for _ in 0..256 {
            if let Some(sample) = source.next() {
                if (sample - 0.9).abs() < 0.01 {
                    found_new_value = true;
                    break;
                }
            }
        }

        assert!(found_new_value, "Buffer should have swapped");
    }
}
