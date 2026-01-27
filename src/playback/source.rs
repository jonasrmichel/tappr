use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use crate::audio::{LoopBuffer, CHANNELS, SAMPLE_RATE};

/// One-shot audio source that plays a buffer once and ends
pub struct OneShotSource {
    /// Audio samples
    samples: Arc<[f32]>,
    /// Current playback position (sample index)
    position: usize,
    /// Total number of samples
    total_samples: usize,
}

impl OneShotSource {
    /// Create a new one-shot source with the given buffer
    pub fn new(buffer: LoopBuffer) -> Self {
        let total_samples = buffer.samples.len();
        Self {
            samples: buffer.samples,
            position: 0,
            total_samples,
        }
    }
}

impl Source for OneShotSource {
    fn current_frame_len(&self) -> Option<usize> {
        // Return remaining frames
        let remaining = self.total_samples.saturating_sub(self.position);
        Some(remaining / CHANNELS as usize)
    }

    fn channels(&self) -> u16 {
        CHANNELS
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.total_samples / CHANNELS as usize;
        let secs = frames as f64 / SAMPLE_RATE as f64;
        Some(Duration::from_secs_f64(secs))
    }
}

impl Iterator for OneShotSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.position >= self.total_samples {
            return None; // End of playback
        }

        let sample = self.samples[self.position];
        self.position += 1;
        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LoopInfo;

    fn create_test_buffer(samples: Vec<f32>) -> LoopBuffer {
        let duration_samples = samples.len() / CHANNELS as usize;
        LoopBuffer::new(
            samples,
            LoopInfo {
                bpm: 120.0,
                source_bpm: 120.0,
                bpm_confidence: 1.0,
                time_stretched: false,
                bars: 1,
                beats_per_bar: 4,
                duration_samples,
                sample_rate: SAMPLE_RATE,
            },
        )
    }

    #[test]
    fn test_one_shot_source_plays_once() {
        let samples = vec![0.1, 0.2, 0.3, 0.4]; // 2 stereo frames
        let buffer = create_test_buffer(samples);

        let mut source = OneShotSource::new(buffer);

        // Read all samples
        let output: Vec<f32> = std::iter::from_fn(|| source.next()).collect();

        // Should play exactly once, no looping
        assert_eq!(output.len(), 4);
        assert_eq!(output, vec![0.1, 0.2, 0.3, 0.4]);

        // Next call should return None
        assert!(source.next().is_none());
    }

    #[test]
    fn test_one_shot_source_duration() {
        let samples = vec![0.0; 96000]; // 1 second at 48kHz stereo
        let buffer = create_test_buffer(samples);

        let source = OneShotSource::new(buffer);

        let duration = source.total_duration().unwrap();
        assert!((duration.as_secs_f64() - 1.0).abs() < 0.01);
    }
}
