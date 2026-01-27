use std::sync::Arc;

use crate::app::LoopInfo;

/// Standard sample rate for internal processing
pub const SAMPLE_RATE: u32 = 48_000;

/// Number of audio channels (stereo)
pub const CHANNELS: u16 = 2;

/// Raw audio buffer before quantization
#[derive(Debug)]
pub struct RawAudioBuffer {
    /// Interleaved samples (f32, normalized to -1.0 to 1.0)
    pub samples: Vec<f32>,
    /// Sample rate of the audio
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
}

impl RawAudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    /// Duration in seconds
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
    }

    /// Number of frames (samples per channel)
    #[allow(dead_code)]
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    /// Convert to mono by averaging channels
    pub fn to_mono(&self) -> Vec<f32> {
        if self.channels == 1 {
            return self.samples.clone();
        }

        self.samples
            .chunks(self.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect()
    }
}

/// Immutable loop buffer for playback
#[derive(Debug, Clone)]
pub struct LoopBuffer {
    /// Interleaved stereo samples (f32, -1.0 to 1.0)
    pub samples: Arc<[f32]>,
    /// Metadata about the loop
    pub loop_info: LoopInfo,
}

impl LoopBuffer {
    pub fn new(samples: Vec<f32>, loop_info: LoopInfo) -> Self {
        Self {
            samples: samples.into(),
            loop_info,
        }
    }

    /// Get sample rate
    #[allow(dead_code)]
    pub fn sample_rate(&self) -> u32 {
        self.loop_info.sample_rate
    }

    /// Duration in seconds
    #[allow(dead_code)]
    pub fn duration_secs(&self) -> f32 {
        self.loop_info.duration_samples as f32 / self.loop_info.sample_rate as f32
    }

    /// Number of frames (samples per channel)
    #[allow(dead_code)]
    pub fn frame_count(&self) -> usize {
        self.samples.len() / CHANNELS as usize
    }

    /// Get the number of samples per bar
    pub fn samples_per_bar(&self) -> usize {
        self.samples.len() / self.loop_info.bars as usize
    }
}
