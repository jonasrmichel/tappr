use ssstretch::Stretch;
use tracing::{debug, instrument};

use super::buffer::{RawAudioBuffer, CHANNELS, SAMPLE_RATE};

/// Time-stretches audio to match a target BPM while preserving pitch
pub struct TimeStretcher {
    sample_rate: f32,
}

impl TimeStretcher {
    pub fn new() -> Self {
        Self {
            sample_rate: SAMPLE_RATE as f32,
        }
    }

    /// Time-stretch audio from source BPM to target BPM
    ///
    /// This changes the tempo without affecting pitch.
    /// For example, stretching 90 BPM to 120 BPM makes the audio play faster.
    #[instrument(skip(self, input))]
    pub fn stretch_to_bpm(
        &self,
        input: &RawAudioBuffer,
        source_bpm: f32,
        target_bpm: f32,
    ) -> RawAudioBuffer {
        // Calculate stretch ratio: how much to speed up or slow down
        // target_bpm > source_bpm means we need to speed up (compress time)
        // target_bpm < source_bpm means we need to slow down (expand time)
        let stretch_ratio = source_bpm / target_bpm;

        debug!(
            source_bpm,
            target_bpm,
            stretch_ratio,
            "Calculating time stretch"
        );

        // If ratio is close to 1.0, skip stretching
        if (stretch_ratio - 1.0).abs() < 0.01 {
            debug!("Stretch ratio near 1.0, skipping time stretch");
            return RawAudioBuffer::new(
                input.samples.clone(),
                input.sample_rate,
                input.channels,
            );
        }

        // Warn if stretch ratio is outside optimal range
        if stretch_ratio < 0.75 || stretch_ratio > 1.5 {
            debug!(
                stretch_ratio,
                "Stretch ratio outside optimal range (0.75-1.5), quality may degrade"
            );
        }

        // Convert interleaved stereo to separate channels for ssstretch
        let (left, right) = self.deinterleave(&input.samples);

        let input_len = left.len() as i32;
        let output_len = (input_len as f32 * stretch_ratio) as i32;

        debug!(
            input_samples = input_len,
            output_samples = output_len,
            "Time stretching audio"
        );

        // Create and configure stretcher for stereo audio
        let mut stretcher = Stretch::new();
        stretcher.preset_default(CHANNELS as i32, self.sample_rate);

        // Prepare input/output buffers
        let input_channels: Vec<Vec<f32>> = vec![left, right];
        let mut output_channels: Vec<Vec<f32>> = vec![
            vec![0.0f32; output_len as usize],
            vec![0.0f32; output_len as usize],
        ];

        // Process the audio
        stretcher.process_vec(&input_channels, input_len, &mut output_channels, output_len);

        // Convert back to interleaved stereo
        let interleaved = self.interleave(&output_channels[0], &output_channels[1]);

        debug!(
            output_samples = interleaved.len(),
            output_duration_secs = interleaved.len() as f32 / (self.sample_rate * CHANNELS as f32),
            "Time stretch complete"
        );

        RawAudioBuffer::new(interleaved, SAMPLE_RATE, CHANNELS)
    }

    /// Deinterleave stereo samples into separate left/right channels
    fn deinterleave(&self, interleaved: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let frame_count = interleaved.len() / 2;
        let mut left = Vec::with_capacity(frame_count);
        let mut right = Vec::with_capacity(frame_count);

        for frame in interleaved.chunks(2) {
            left.push(frame[0]);
            right.push(frame.get(1).copied().unwrap_or(frame[0]));
        }

        (left, right)
    }

    /// Interleave separate left/right channels into stereo samples
    fn interleave(&self, left: &[f32], right: &[f32]) -> Vec<f32> {
        let mut interleaved = Vec::with_capacity(left.len() * 2);

        for (l, r) in left.iter().zip(right.iter()) {
            interleaved.push(*l);
            interleaved.push(*r);
        }

        interleaved
    }
}

impl Default for TimeStretcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deinterleave_interleave_roundtrip() {
        let stretcher = TimeStretcher::new();
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];

        let (left, right) = stretcher.deinterleave(&original);
        assert_eq!(left, vec![1.0, 3.0, 5.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0]);

        let roundtrip = stretcher.interleave(&left, &right);
        assert_eq!(roundtrip, original);
    }

    #[test]
    fn test_stretch_ratio_calculation() {
        // 90 BPM -> 120 BPM: ratio = 90/120 = 0.75 (speed up, shorter output)
        let ratio: f32 = 90.0 / 120.0;
        assert!((ratio - 0.75).abs() < 0.001);

        // 150 BPM -> 120 BPM: ratio = 150/120 = 1.25 (slow down, longer output)
        let ratio: f32 = 150.0 / 120.0;
        assert!((ratio - 1.25).abs() < 0.001);
    }

    #[test]
    fn test_stretch_skip_near_unity() {
        let stretcher = TimeStretcher::new();
        let samples = vec![0.5; 1000];
        let input = RawAudioBuffer::new(samples.clone(), SAMPLE_RATE, CHANNELS);

        // Stretch ratio of 1.0 should return unchanged audio
        let output = stretcher.stretch_to_bpm(&input, 120.0, 120.0);
        assert_eq!(output.samples.len(), input.samples.len());
    }
}
