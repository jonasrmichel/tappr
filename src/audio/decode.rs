use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::{debug, error, instrument};

use crate::error::AudioError;

use super::buffer::{RawAudioBuffer, CHANNELS, SAMPLE_RATE};

/// Audio decoder using ffmpeg subprocess
pub struct AudioDecoder;

impl AudioDecoder {
    /// Decode raw stream bytes to PCM samples using ffmpeg
    #[instrument(skip(input))]
    pub async fn decode(input: &[u8]) -> Result<RawAudioBuffer, AudioError> {
        debug!(input_bytes = input.len(), "Decoding audio");

        // Check if ffmpeg is available
        let ffmpeg_check = Command::new("ffmpeg").arg("-version").output().await;

        if ffmpeg_check.is_err() {
            return Err(AudioError::FfmpegNotFound);
        }

        // Build ffmpeg command
        // Input: pipe:0 (stdin)
        // Output: f32le PCM at 48kHz stereo
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                "pipe:0", // Input from stdin
                "-f",
                "f32le", // Output format: 32-bit float little-endian
                "-acodec",
                "pcm_f32le", // PCM codec
                "-ar",
                &SAMPLE_RATE.to_string(), // Sample rate
                "-ac",
                &CHANNELS.to_string(), // Channels (stereo)
                "pipe:1", // Output to stdout
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AudioError::FfmpegError(format!("Failed to spawn ffmpeg: {}", e)))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AudioError::FfmpegError("Failed to get stdin".into()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| AudioError::FfmpegError("Failed to get stdout".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| AudioError::FfmpegError("Failed to get stderr".into()))?;

        // Write input in a separate task to avoid deadlock
        let input_data = input.to_vec();
        let write_handle = tokio::spawn(async move {
            if let Err(e) = stdin.write_all(&input_data).await {
                error!(error = %e, "Failed to write to ffmpeg stdin");
            }
            drop(stdin); // Close stdin to signal EOF
        });

        // Read stdout and stderr concurrently
        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        let (stdout_result, stderr_result) =
            tokio::join!(stdout.read_to_end(&mut output), stderr.read_to_end(&mut stderr_output),);

        stdout_result.map_err(|e| AudioError::FfmpegError(format!("Failed to read stdout: {}", e)))?;
        stderr_result.map_err(|e| AudioError::FfmpegError(format!("Failed to read stderr: {}", e)))?;

        // Wait for write to complete
        let _ = write_handle.await;

        // Wait for ffmpeg to exit
        let status = child.wait().await?;

        if !status.success() {
            let stderr_str = String::from_utf8_lossy(&stderr_output);
            error!(stderr = %stderr_str, "ffmpeg failed");
            return Err(AudioError::FfmpegFailed(status));
        }

        if output.is_empty() {
            return Err(AudioError::DecodeError("ffmpeg produced no output".into()));
        }

        // Convert bytes to f32 samples
        // Each sample is 4 bytes (f32 little-endian)
        let samples: Vec<f32> = output
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();

        debug!(
            output_samples = samples.len(),
            duration_secs = samples.len() as f32 / (SAMPLE_RATE as f32 * CHANNELS as f32),
            "Decode complete"
        );

        Ok(RawAudioBuffer::new(samples, SAMPLE_RATE, CHANNELS))
    }
}
