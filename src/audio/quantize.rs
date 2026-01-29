use tracing::{debug, info, instrument};

use crate::app::{BpmMode, LoopInfo};
use crate::error::AudioError;

use super::buffer::{LoopBuffer, RawAudioBuffer, CHANNELS, SAMPLE_RATE};
use super::stretch::TimeStretcher;

/// BPM estimation result
struct BpmEstimate {
    bpm: f32,
    confidence: f32,
}

/// Audio quantizer for beat alignment with time-stretching
#[derive(Clone)]
pub struct Quantizer {
    min_bpm: f32,
    max_bpm: f32,
    stretcher: TimeStretcher,
}

impl Quantizer {
    pub fn new(min_bpm: f32, max_bpm: f32) -> Self {
        Self {
            min_bpm,
            max_bpm,
            stretcher: TimeStretcher::new(),
        }
    }

    /// Quantize raw audio to a loopable buffer with tempo matching
    ///
    /// This method:
    /// 1. Detects the BPM of the source audio
    /// 2. Time-stretches to match the target BPM (if using Fixed mode)
    /// 3. Extracts a beat-aligned loop segment
    #[instrument(skip(self, raw))]
    pub fn quantize(
        &self,
        raw: RawAudioBuffer,
        bpm_mode: BpmMode,
        bars: u8,
        beats_per_bar: u8,
    ) -> Result<LoopBuffer, AudioError> {
        debug!(
            input_samples = raw.samples.len(),
            duration_secs = raw.duration_secs(),
            "Starting quantization"
        );

        // Step 1: Always detect source BPM first
        let source_estimate = self.detect_bpm(&raw, self.min_bpm, self.max_bpm);
        let source_bpm = source_estimate.bpm;
        let detection_confidence = source_estimate.confidence;

        debug!(
            source_bpm,
            confidence = detection_confidence,
            "Detected source BPM"
        );

        // Step 2: Determine target BPM and whether to time-stretch
        let (target_bpm, confidence, audio_to_process) = match bpm_mode {
            BpmMode::Fixed(fixed_bpm) => {
                // Time-stretch to match the fixed target BPM
                info!(
                    source_bpm,
                    target_bpm = fixed_bpm,
                    "Time-stretching to fixed BPM"
                );

                let stretched = self.stretcher.stretch_to_bpm(&raw, source_bpm, fixed_bpm);

                debug!(
                    original_samples = raw.samples.len(),
                    stretched_samples = stretched.samples.len(),
                    "Time stretch complete"
                );

                (fixed_bpm, 1.0, stretched)
            }
            BpmMode::Auto { .. } => {
                // Use detected BPM, no time-stretching needed
                debug!(source_bpm, "Using detected BPM (no time-stretch)");
                (source_bpm, detection_confidence, raw)
            }
        };

        // Step 3: Calculate target loop length in samples (at the target BPM)
        let total_beats = bars as u32 * beats_per_bar as u32;
        let beat_duration_secs = 60.0 / target_bpm;
        let loop_duration_secs = beat_duration_secs * total_beats as f32;
        let target_frames = (loop_duration_secs * SAMPLE_RATE as f32) as usize;
        let target_samples = target_frames * CHANNELS as usize;

        debug!(
            total_beats,
            loop_duration_secs,
            target_frames,
            "Calculated loop length"
        );

        // Verify we have enough audio after stretching
        if audio_to_process.samples.len() < target_samples {
            return Err(AudioError::AudioTooShort(loop_duration_secs));
        }

        // Step 4: Find best starting point using onset detection
        let onsets = self.detect_onsets(&audio_to_process);
        let start_sample = self.find_best_start(&audio_to_process, &onsets, target_samples);

        debug!(
            start_sample,
            start_frame = start_sample / CHANNELS as usize,
            "Selected start point"
        );

        // Step 5: Extract loop segment
        let end_sample = start_sample + target_samples;
        let mut samples = audio_to_process.samples[start_sample..end_sample].to_vec();

        // Step 6: Apply crossfade at loop boundaries to prevent clicks
        self.apply_fades(&mut samples);

        // Build result
        let time_stretched = matches!(bpm_mode, BpmMode::Fixed(_)) && (source_bpm - target_bpm).abs() > 0.5;
        let loop_info = LoopInfo {
            bpm: target_bpm,
            source_bpm,
            bpm_confidence: confidence,
            time_stretched,
            bars,
            beats_per_bar,
            duration_samples: target_frames,
            sample_rate: SAMPLE_RATE,
        };

        info!(
            source_bpm,
            target_bpm = loop_info.bpm,
            bars = loop_info.bars,
            duration_secs = loop_duration_secs,
            time_stretched,
            "Quantization complete"
        );

        Ok(LoopBuffer::new(samples, loop_info))
    }

    /// BPM detection using energy envelope and autocorrelation
    fn detect_bpm(&self, raw: &RawAudioBuffer, min_bpm: f32, max_bpm: f32) -> BpmEstimate {
        // Convert to mono for analysis
        let mono = raw.to_mono();

        // Compute energy envelope using windowed RMS
        let window_size = raw.sample_rate as usize / 20; // 50ms windows
        let hop_size = window_size / 2;

        let envelope: Vec<f32> = mono
            .chunks(hop_size)
            .map(|chunk| {
                let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
                (sum_sq / chunk.len() as f32).sqrt()
            })
            .collect();

        if envelope.len() < 2 {
            return BpmEstimate {
                bpm: 120.0,
                confidence: 0.0,
            };
        }

        // Convert BPM range to lag range (in envelope samples)
        let envelope_rate = raw.sample_rate as f32 / hop_size as f32;
        let min_lag = (60.0 / max_bpm * envelope_rate) as usize;
        let max_lag = (60.0 / min_bpm * envelope_rate) as usize;

        // Find best correlation lag
        let mut best_lag = min_lag;
        let mut best_correlation = f32::NEG_INFINITY;

        for lag in min_lag..=max_lag.min(envelope.len() / 2) {
            let correlation = self.autocorrelate(&envelope, lag);
            if correlation > best_correlation {
                best_correlation = correlation;
                best_lag = lag;
            }
        }

        // Convert lag back to BPM
        let bpm = 60.0 / (best_lag as f32 / envelope_rate);

        // Normalize confidence (correlation can be negative)
        let confidence = (best_correlation + 1.0) / 2.0;

        BpmEstimate {
            bpm: bpm.clamp(min_bpm, max_bpm),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// Compute autocorrelation at a specific lag
    fn autocorrelate(&self, signal: &[f32], lag: usize) -> f32 {
        let n = signal.len() - lag;
        if n == 0 {
            return 0.0;
        }

        let sum: f32 = signal[..n]
            .iter()
            .zip(&signal[lag..])
            .map(|(a, b)| a * b)
            .sum();

        sum / n as f32
    }

    /// Simple onset detection using energy increases
    fn detect_onsets(&self, raw: &RawAudioBuffer) -> Vec<usize> {
        let mono = raw.to_mono();

        let hop_size = 512;
        let threshold = 1.5;

        let mut onsets = Vec::new();
        let mut prev_energy = 0.0f32;

        for (i, chunk) in mono.chunks(hop_size).enumerate() {
            let energy: f32 = chunk.iter().map(|s| s.abs()).sum::<f32>() / chunk.len() as f32;

            if energy > prev_energy * threshold && energy > 0.01 {
                // Convert to stereo sample index
                onsets.push(i * hop_size * CHANNELS as usize);
            }
            prev_energy = energy.max(0.001); // Avoid division by zero
        }

        debug!(onset_count = onsets.len(), "Detected onsets");
        onsets
    }

    /// Find best starting point aligned to an onset
    fn find_best_start(&self, raw: &RawAudioBuffer, onsets: &[usize], target_len: usize) -> usize {
        let max_start = raw.samples.len().saturating_sub(target_len);

        // Prefer starting near an onset, but not too close to the end
        for &onset in onsets {
            if onset < max_start {
                return onset;
            }
        }

        // Fallback: start from beginning
        0
    }

    /// Apply fade in/out to prevent clicks at loop boundaries
    fn apply_fades(&self, samples: &mut [f32]) {
        // ~21ms fade at 48kHz stereo
        let fade_samples = 2048.min(samples.len() / 4);

        // Fade in (beginning of loop)
        for i in 0..fade_samples {
            let gain = i as f32 / fade_samples as f32;
            // Apply smooth curve (raised cosine)
            let gain = 0.5 * (1.0 - (std::f32::consts::PI * gain).cos());
            samples[i] *= gain;
        }

        // Fade out (end of loop)
        let len = samples.len();
        for i in 0..fade_samples {
            let gain = i as f32 / fade_samples as f32;
            // Apply smooth curve (raised cosine)
            let gain = 0.5 * (1.0 - (std::f32::consts::PI * gain).cos());
            samples[len - 1 - i] *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantizer_fixed_bpm() {
        let quantizer = Quantizer::new(70.0, 170.0);

        // Create a simple 10-second buffer
        let sample_rate = 48000;
        let channels = 2;
        let duration_secs = 10.0;
        let num_samples = (duration_secs * sample_rate as f32 * channels as f32) as usize;

        // Generate a simple sine wave
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (440.0 * 2.0 * std::f32::consts::PI * t).sin() * 0.5
            })
            .collect();

        let raw = RawAudioBuffer::new(samples, sample_rate, channels);

        // Quantize with fixed BPM
        let result = quantizer.quantize(raw, BpmMode::Fixed(120.0), 2, 4);

        assert!(result.is_ok());
        let loop_buffer = result.unwrap();

        assert_eq!(loop_buffer.loop_info.bpm, 120.0);
        assert!(loop_buffer.loop_info.time_stretched); // Fixed BPM triggers time stretch
        assert_eq!(loop_buffer.loop_info.bars, 2);
        assert_eq!(loop_buffer.loop_info.beats_per_bar, 4);

        // 2 bars * 4 beats/bar = 8 beats at 120 BPM = 4 seconds = 192000 frames
        let expected_frames = (4.0 * sample_rate as f32) as usize;
        assert_eq!(loop_buffer.loop_info.duration_samples, expected_frames);
    }

    #[test]
    fn test_bpm_detection_click_track() {
        let quantizer = Quantizer::new(60.0, 180.0);

        // Generate a click track at 120 BPM (0.5s between clicks)
        let sample_rate = 48000;
        let channels = 2;
        let duration_secs = 8.0;
        let num_samples = (duration_secs * sample_rate as f32 * channels as f32) as usize;

        let click_interval_samples = (0.5 * sample_rate as f32) as usize * channels as usize;
        let click_length = 100 * channels as usize;

        let mut samples = vec![0.0f32; num_samples];

        // Add clicks
        let mut pos = 0;
        while pos < num_samples - click_length {
            for i in 0..click_length {
                samples[pos + i] = 0.8;
            }
            pos += click_interval_samples;
        }

        let raw = RawAudioBuffer::new(samples, sample_rate, channels);

        // Detect BPM
        let estimate = quantizer.detect_bpm(&raw, 60.0, 180.0);

        // Should be close to 120 BPM
        assert!(
            (estimate.bpm - 120.0).abs() < 5.0,
            "Expected BPM ~120, got {}",
            estimate.bpm
        );
        assert!(
            estimate.confidence > 0.3,
            "Expected confidence > 0.3, got {}",
            estimate.confidence
        );
    }
}
