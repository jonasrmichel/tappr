use tracing::{debug, info, instrument};

use super::buffer::RawAudioBuffer;

/// Content type classification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentType {
    /// Likely music content
    Music,
    /// Likely speech/talk content
    Speech,
    /// Silence or very low audio
    Silence,
    /// Unable to classify confidently
    Unknown,
}

impl ContentType {
    /// Returns true if content is likely music or at least not clearly non-music
    /// We give uncertain content the benefit of the doubt to avoid rejecting good stations
    pub fn is_music(&self) -> bool {
        matches!(self, ContentType::Music | ContentType::Unknown)
    }
}

/// Classification result with confidence
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClassificationResult {
    pub content_type: ContentType,
    pub confidence: f32,
    pub details: ClassificationDetails,
}

/// Detailed metrics from classification
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClassificationDetails {
    pub rms: f32,
    pub zero_crossing_rate: f32,
    pub zcr_variance: f32,
    pub spectral_flatness: f32,
    pub energy_variance: f32,
    pub silent_ratio: f32,
}

/// Audio content classifier using spectral analysis
pub struct AudioClassifier {
    /// RMS threshold below which audio is considered silence
    silence_threshold: f32,
    /// Ratio of silent frames that indicates mostly silence
    silence_ratio_threshold: f32,
    /// ZCR variance threshold - speech has higher variance
    zcr_variance_threshold: f32,
    /// Spectral flatness threshold - speech is more "noisy"
    spectral_flatness_threshold: f32,
}

impl Default for AudioClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioClassifier {
    pub fn new() -> Self {
        Self {
            silence_threshold: 0.01,        // -40dB
            silence_ratio_threshold: 0.5,    // >50% silence = skip
            zcr_variance_threshold: 0.15,    // High variance = speech
            spectral_flatness_threshold: 0.3, // High flatness = noise/speech
        }
    }

    /// Classify audio content
    #[instrument(skip(self, audio))]
    pub fn classify(&self, audio: &RawAudioBuffer) -> ClassificationResult {
        let mono = audio.to_mono();

        if mono.is_empty() {
            return ClassificationResult {
                content_type: ContentType::Silence,
                confidence: 1.0,
                details: ClassificationDetails {
                    rms: 0.0,
                    zero_crossing_rate: 0.0,
                    zcr_variance: 0.0,
                    spectral_flatness: 0.0,
                    energy_variance: 0.0,
                    silent_ratio: 1.0,
                },
            };
        }

        // Calculate features
        let rms = self.calculate_rms(&mono);
        let (zcr, zcr_variance) = self.calculate_zcr_stats(&mono);
        let spectral_flatness = self.calculate_spectral_flatness(&mono);
        let energy_variance = self.calculate_energy_variance(&mono);
        let silent_ratio = self.calculate_silent_ratio(&mono);

        let details = ClassificationDetails {
            rms,
            zero_crossing_rate: zcr,
            zcr_variance,
            spectral_flatness,
            energy_variance,
            silent_ratio,
        };

        debug!(
            rms,
            zcr,
            zcr_variance,
            spectral_flatness,
            energy_variance,
            silent_ratio,
            "Audio features calculated"
        );

        // Classification logic
        let (content_type, confidence) = self.classify_from_features(&details);

        info!(
            content_type = ?content_type,
            confidence,
            "Content classified"
        );

        ClassificationResult {
            content_type,
            confidence,
            details,
        }
    }

    /// Classify based on calculated features
    fn classify_from_features(&self, details: &ClassificationDetails) -> (ContentType, f32) {
        // Check for silence first
        if details.rms < self.silence_threshold || details.silent_ratio > self.silence_ratio_threshold {
            return (ContentType::Silence, 0.9);
        }

        // Speech indicators:
        // - High ZCR variance (speech has varied pacing)
        // - High spectral flatness (speech is more "noisy")
        // - High energy variance (pauses between words/sentences)
        let speech_score = self.calculate_speech_score(details);
        let music_score = self.calculate_music_score(details);

        debug!(speech_score, music_score, "Classification scores");

        if speech_score > 0.6 && speech_score > music_score {
            let confidence = (speech_score - music_score).min(0.5) + 0.5;
            (ContentType::Speech, confidence)
        } else if music_score > 0.5 {
            let confidence = (music_score - speech_score).max(0.0).min(0.4) + 0.6;
            (ContentType::Music, confidence)
        } else {
            (ContentType::Unknown, 0.5)
        }
    }

    /// Calculate speech likelihood score (0.0 - 1.0)
    fn calculate_speech_score(&self, details: &ClassificationDetails) -> f32 {
        let mut score: f32 = 0.0;

        // High ZCR variance indicates speech
        if details.zcr_variance > self.zcr_variance_threshold {
            score += 0.35;
        } else if details.zcr_variance > self.zcr_variance_threshold * 0.5 {
            score += 0.15;
        }

        // High spectral flatness indicates speech/noise
        if details.spectral_flatness > self.spectral_flatness_threshold {
            score += 0.35;
        } else if details.spectral_flatness > self.spectral_flatness_threshold * 0.7 {
            score += 0.15;
        }

        // High energy variance indicates speech (pauses)
        if details.energy_variance > 0.3 {
            score += 0.3;
        } else if details.energy_variance > 0.15 {
            score += 0.15;
        }

        score.min(1.0)
    }

    /// Calculate music likelihood score (0.0 - 1.0)
    fn calculate_music_score(&self, details: &ClassificationDetails) -> f32 {
        let mut score: f32 = 0.0;

        // Low ZCR variance indicates consistent content (music)
        if details.zcr_variance < self.zcr_variance_threshold * 0.5 {
            score += 0.3;
        }

        // Lower spectral flatness indicates tonal content (music)
        if details.spectral_flatness < self.spectral_flatness_threshold * 0.5 {
            score += 0.3;
        }

        // Moderate energy variance (music has dynamics but not speech-like pauses)
        if details.energy_variance > 0.05 && details.energy_variance < 0.25 {
            score += 0.2;
        }

        // Good RMS level indicates active audio
        if details.rms > 0.05 {
            score += 0.2;
        }

        // Low silent ratio
        if details.silent_ratio < 0.1 {
            score += 0.2;
        }

        score.min(1.0)
    }

    /// Calculate RMS (root mean square) energy
    fn calculate_rms(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    /// Calculate zero-crossing rate and its variance
    fn calculate_zcr_stats(&self, samples: &[f32]) -> (f32, f32) {
        if samples.len() < 2 {
            return (0.0, 0.0);
        }

        // Calculate ZCR in windows
        let window_size = (samples.len() / 50).max(1024); // ~50 windows
        let mut zcr_values = Vec::new();

        for chunk in samples.chunks(window_size) {
            if chunk.len() < 2 {
                continue;
            }
            let crossings: usize = chunk
                .windows(2)
                .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                .count();
            let zcr = crossings as f32 / chunk.len() as f32;
            zcr_values.push(zcr);
        }

        if zcr_values.is_empty() {
            return (0.0, 0.0);
        }

        let mean_zcr: f32 = zcr_values.iter().sum::<f32>() / zcr_values.len() as f32;
        let variance: f32 = zcr_values
            .iter()
            .map(|z| (z - mean_zcr).powi(2))
            .sum::<f32>()
            / zcr_values.len() as f32;

        (mean_zcr, variance.sqrt()) // Return std dev, not variance
    }

    /// Calculate spectral flatness (Wiener entropy)
    /// Higher values indicate noise-like content, lower values indicate tonal content
    fn calculate_spectral_flatness(&self, samples: &[f32]) -> f32 {
        // Use a simple approximation based on amplitude distribution
        // Real spectral flatness would require FFT

        if samples.is_empty() {
            return 0.0;
        }

        // Calculate histogram of absolute amplitudes
        let abs_samples: Vec<f32> = samples.iter().map(|s| s.abs()).collect();
        let max_amp = abs_samples.iter().fold(0.0f32, |a, &b| a.max(b));

        if max_amp < 0.001 {
            return 1.0; // Silence is maximally flat
        }

        // Normalize and calculate entropy-like measure
        let normalized: Vec<f32> = abs_samples.iter().map(|s| s / max_amp).collect();

        // Calculate how "spread out" the amplitudes are
        // Tonal content has peaks, noise has flat distribution
        let bins = 20;
        let mut histogram = vec![0usize; bins];

        for &s in &normalized {
            let bin = ((s * (bins - 1) as f32) as usize).min(bins - 1);
            histogram[bin] += 1;
        }

        // Calculate entropy
        let total = normalized.len() as f32;
        let entropy: f32 = histogram
            .iter()
            .filter(|&&count| count > 0)
            .map(|&count| {
                let p = count as f32 / total;
                -p * p.ln()
            })
            .sum();

        // Normalize entropy to 0-1 range
        let max_entropy = (bins as f32).ln();
        (entropy / max_entropy).min(1.0)
    }

    /// Calculate energy variance across frames
    fn calculate_energy_variance(&self, samples: &[f32]) -> f32 {
        let frame_size = (samples.len() / 100).max(512); // ~100 frames
        let mut energies = Vec::new();

        for chunk in samples.chunks(frame_size) {
            let energy: f32 = chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32;
            energies.push(energy);
        }

        if energies.is_empty() {
            return 0.0;
        }

        let mean_energy: f32 = energies.iter().sum::<f32>() / energies.len() as f32;
        if mean_energy < 0.0001 {
            return 0.0;
        }

        let variance: f32 = energies
            .iter()
            .map(|e| (e - mean_energy).powi(2))
            .sum::<f32>()
            / energies.len() as f32;

        // Normalize by mean energy to get coefficient of variation
        (variance.sqrt() / mean_energy).min(1.0)
    }

    /// Calculate ratio of silent frames
    fn calculate_silent_ratio(&self, samples: &[f32]) -> f32 {
        let frame_size = (samples.len() / 100).max(512);
        let mut silent_frames = 0;
        let mut total_frames = 0;

        for chunk in samples.chunks(frame_size) {
            let rms: f32 = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            if rms < self.silence_threshold {
                silent_frames += 1;
            }
            total_frames += 1;
        }

        if total_frames == 0 {
            return 1.0;
        }

        silent_frames as f32 / total_frames as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_audio(samples: Vec<f32>) -> RawAudioBuffer {
        RawAudioBuffer::new(samples, 48000, 1)
    }

    #[test]
    fn test_silence_detection() {
        let classifier = AudioClassifier::new();

        // Near-silent audio
        let silent = create_test_audio(vec![0.001; 48000]);
        let result = classifier.classify(&silent);

        assert_eq!(result.content_type, ContentType::Silence);
    }

    #[test]
    fn test_classify_tone() {
        let classifier = AudioClassifier::new();

        // Generate a pure sine wave (very tonal = music-like)
        let samples: Vec<f32> = (0..48000)
            .map(|i| (440.0 * 2.0 * std::f32::consts::PI * i as f32 / 48000.0).sin() * 0.5)
            .collect();
        let tonal = create_test_audio(samples);
        let result = classifier.classify(&tonal);

        // Pure tone should be classified as music (tonal content)
        assert!(
            result.content_type == ContentType::Music || result.content_type == ContentType::Unknown,
            "Expected Music or Unknown, got {:?}",
            result.content_type
        );
    }

    #[test]
    fn test_classify_noise() {
        let classifier = AudioClassifier::new();

        // Generate noise-like content with high ZCR variance
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let samples: Vec<f32> = (0..48000)
            .map(|_| rng.gen_range(-0.3..0.3))
            .collect();
        let noise = create_test_audio(samples);
        let result = classifier.classify(&noise);

        // Noise should likely be classified as speech or unknown (not music)
        assert!(
            result.content_type != ContentType::Silence,
            "Noise should not be classified as silence"
        );
    }
}
