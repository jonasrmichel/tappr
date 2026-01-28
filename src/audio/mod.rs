mod buffer;
mod decode;
mod quantize;
mod stream;
mod stretch;

pub use buffer::{LoopBuffer, CHANNELS, SAMPLE_RATE};
pub use decode::AudioDecoder;
pub use quantize::Quantizer;
pub use stream::StreamCapture;

// TimeStretcher is used internally by Quantizer
#[allow(unused_imports)]
pub use stretch::TimeStretcher;

use tracing::{debug, error, instrument};

use crate::app::{BpmMode, StationInfo};
use crate::error::AudioError;

/// Audio processing pipeline
pub struct AudioPipeline {
    stream_capture: StreamCapture,
    quantizer: Quantizer,
}

impl AudioPipeline {
    pub fn new(min_bpm: f32, max_bpm: f32) -> Self {
        Self {
            stream_capture: StreamCapture::new(),
            quantizer: Quantizer::new(min_bpm, max_bpm),
        }
    }

    /// Quick-start processing for immediate playback (first station only)
    /// Uses shorter capture time and skips time-stretching for fast startup
    #[instrument(skip(self, station), fields(station_name = %station.name))]
    pub async fn process_station_quick(
        &self,
        station: &StationInfo,
        beats_per_bar: u8,
    ) -> Result<LoopBuffer, AudioError> {
        const QUICK_LISTEN_SECONDS: u32 = 6;  // Just enough for 4 bars at slow tempo
        const QUICK_BARS: u8 = 4;

        let stream_url = station
            .stream_url
            .as_ref()
            .ok_or_else(|| AudioError::DecodeError("No stream URL".into()))?;

        debug!(stream_url, "Quick-start audio pipeline");

        // Step 1: Quick capture (6 seconds)
        let raw_bytes = self.stream_capture.capture(stream_url, QUICK_LISTEN_SECONDS).await?;
        debug!(bytes = raw_bytes.len(), "Quick capture complete");

        // Step 2: Decode to PCM
        let raw_audio = AudioDecoder::decode(&raw_bytes).await?;
        debug!(
            samples = raw_audio.samples.len(),
            duration_secs = raw_audio.duration_secs(),
            "Audio decoded"
        );

        // Step 3: Quantize with Auto BPM (no time-stretching for speed)
        let loop_buffer = self
            .quantizer
            .quantize(raw_audio, BpmMode::Auto { min: 70.0, max: 170.0 }, QUICK_BARS, beats_per_bar)?;
        debug!(
            bpm = loop_buffer.loop_info.bpm,
            bars = loop_buffer.loop_info.bars,
            "Quick quantization complete"
        );

        Ok(loop_buffer)
    }

    /// Process a station: capture stream, decode, and quantize
    #[instrument(skip(self, station), fields(station_name = %station.name))]
    pub async fn process_station(
        &self,
        station: &StationInfo,
        listen_seconds: u32,
        bpm_mode: BpmMode,
        bars: u8,
        beats_per_bar: u8,
    ) -> Result<LoopBuffer, AudioError> {
        let stream_url = station
            .stream_url
            .as_ref()
            .ok_or_else(|| AudioError::DecodeError("No stream URL".into()))?;

        debug!(stream_url, listen_seconds, "Starting audio pipeline");

        // Step 1: Capture stream
        let raw_bytes = self.stream_capture.capture(stream_url, listen_seconds).await?;
        debug!(bytes = raw_bytes.len(), "Stream captured");

        // Step 2: Decode to PCM
        let raw_audio = AudioDecoder::decode(&raw_bytes).await?;
        debug!(
            samples = raw_audio.samples.len(),
            duration_secs = raw_audio.duration_secs(),
            "Audio decoded"
        );

        // Step 3: Quantize to loop
        let loop_buffer = self
            .quantizer
            .quantize(raw_audio, bpm_mode, bars, beats_per_bar)?;
        debug!(
            bpm = loop_buffer.loop_info.bpm,
            confidence = loop_buffer.loop_info.bpm_confidence,
            bars = loop_buffer.loop_info.bars,
            "Audio quantized"
        );

        Ok(loop_buffer)
    }

    /// Process with retry logic
    #[allow(dead_code)]
    #[instrument(skip(self, station), fields(station_name = %station.name))]
    pub async fn process_station_with_retry(
        &self,
        station: &StationInfo,
        listen_seconds: u32,
        bpm_mode: BpmMode,
        bars: u8,
        beats_per_bar: u8,
        max_retries: u32,
    ) -> Result<LoopBuffer, AudioError> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self
                .process_station(station, listen_seconds, bpm_mode, bars, beats_per_bar)
                .await
            {
                Ok(buffer) => return Ok(buffer),
                Err(e) => {
                    error!(attempt, error = %e, "Audio processing failed");
                    last_error = Some(e);

                    // Wait before retry
                    if attempt < max_retries - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AudioError::DecodeError("Unknown error".into())))
    }
}
