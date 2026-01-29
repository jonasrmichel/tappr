use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use crate::audio::{LoopBuffer, CHANNELS, SAMPLE_RATE};

/// Duration of crossfade in seconds
const CROSSFADE_DURATION_SECS: f32 = 0.5;

/// One-shot audio source that plays a buffer once with crossfade support
pub struct OneShotSource {
    /// Audio samples
    samples: Arc<[f32]>,
    /// Current playback position (sample index)
    position: usize,
    /// Total number of samples
    total_samples: usize,
    /// Number of samples in fade region (per channel)
    fade_samples: usize,
    /// Whether to apply fade-in at start
    fade_in: bool,
    /// Whether to apply fade-out at end
    fade_out: bool,
}

impl OneShotSource {
    /// Create a new one-shot source with crossfade applied
    pub fn new(buffer: LoopBuffer) -> Self {
        Self::with_fades(buffer, true, true)
    }

    /// Create a new one-shot source with configurable fades
    pub fn with_fades(buffer: LoopBuffer, fade_in: bool, fade_out: bool) -> Self {
        let total_samples = buffer.samples.len();
        // Calculate fade samples (total samples for both channels)
        let fade_samples = (CROSSFADE_DURATION_SECS * SAMPLE_RATE as f32 * CHANNELS as f32) as usize;
        // Ensure fade isn't longer than half the clip
        let fade_samples = fade_samples.min(total_samples / 2);

        Self {
            samples: buffer.samples,
            position: 0,
            total_samples,
            fade_samples,
            fade_in,
            fade_out,
        }
    }

    /// Calculate equal-power fade-in gain for a position within the fade region
    /// Uses sine curve for perceptually constant loudness
    fn fade_in_gain(&self, position: usize) -> f32 {
        if !self.fade_in || position >= self.fade_samples {
            return 1.0;
        }
        // Equal-power fade-in: sin(t * π/2) where t goes from 0 to 1
        let t = position as f32 / self.fade_samples as f32;
        (t * std::f32::consts::FRAC_PI_2).sin()
    }

    /// Calculate equal-power fade-out gain for a position within the fade region
    /// Uses cosine curve for perceptually constant loudness
    fn fade_out_gain(&self, position: usize) -> f32 {
        if !self.fade_out {
            return 1.0;
        }
        let fade_start = self.total_samples.saturating_sub(self.fade_samples);
        if position < fade_start {
            return 1.0;
        }
        // Equal-power fade-out: cos(t * π/2) where t goes from 0 to 1
        let t = (position - fade_start) as f32 / self.fade_samples as f32;
        (t * std::f32::consts::FRAC_PI_2).cos()
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

        // Apply crossfade gains
        let fade_in_gain = self.fade_in_gain(self.position);
        let fade_out_gain = self.fade_out_gain(self.position);
        let gain = fade_in_gain * fade_out_gain;

        self.position += 1;
        Some(sample * gain)
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

        // Use no fades for exact sample comparison
        let mut source = OneShotSource::with_fades(buffer, false, false);

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

    #[test]
    fn test_crossfade_applies() {
        // Create a 1-second buffer of constant 1.0 samples
        let samples = vec![1.0; 96000]; // 1 second at 48kHz stereo
        let buffer = create_test_buffer(samples);

        let mut source = OneShotSource::new(buffer);

        // First sample should be near 0 (fade in starts at 0)
        let first = source.next().unwrap();
        assert!(first < 0.1, "First sample should be faded in, got {}", first);

        // Skip to middle (well past fade-in, before fade-out)
        // Fade is 0.5s = 24000 stereo samples = 48000 total samples
        // Middle is around 48000
        for _ in 0..47998 {
            source.next();
        }
        let middle = source.next().unwrap();
        assert!((middle - 1.0).abs() < 0.01, "Middle sample should be ~1.0, got {}", middle);

        // Skip to near the end (in fade-out region)
        for _ in 0..47990 {
            source.next();
        }

        // Last few samples should be faded out
        let mut last = 0.0;
        while let Some(s) = source.next() {
            last = s;
        }
        assert!(last < 0.1, "Last sample should be faded out, got {}", last);
    }

    #[test]
    fn test_equal_power_crossfade() {
        // Verify equal-power property: fade_in^2 + fade_out^2 ≈ 1
        // This ensures constant perceived loudness during crossfade
        let samples = vec![1.0; 96000];
        let buffer = create_test_buffer(samples);

        let source = OneShotSource::new(buffer);

        // Check at various points in fade region
        for i in 0..source.fade_samples {
            let fade_in = source.fade_in_gain(i);
            // Corresponding fade-out position (mirrored)
            let fade_out_pos = source.total_samples - source.fade_samples + i;
            let fade_out = source.fade_out_gain(fade_out_pos);

            // sin^2 + cos^2 = 1 for equal power
            let power_sum = fade_in * fade_in + fade_out * fade_out;
            assert!(
                (power_sum - 1.0).abs() < 0.01,
                "Equal power not maintained at position {}: {} + {} = {}",
                i, fade_in * fade_in, fade_out * fade_out, power_sum
            );
        }
    }
}
