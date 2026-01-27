use std::time::Duration;

use futures::StreamExt;
use reqwest::Client;
use tokio::time::timeout;
use tracing::{debug, instrument, warn};

use crate::error::AudioError;

/// Capture audio stream from HTTP URL
pub struct StreamCapture {
    client: Client,
}

impl StreamCapture {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client for streaming");

        Self { client }
    }

    /// Capture a specified duration of audio from stream URL
    #[instrument(skip(self))]
    pub async fn capture(&self, url: &str, duration_secs: u32) -> Result<Vec<u8>, AudioError> {
        debug!(url, duration_secs, "Starting stream capture");

        let response = self.client.get(url).send().await.map_err(AudioError::StreamError)?;

        if !response.status().is_success() {
            return Err(AudioError::StreamHttpError(response.status()));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");
        debug!(content_type, "Stream content type");

        let mut stream = response.bytes_stream();

        // Estimate buffer size: assume ~256kbps bitrate
        let estimated_size = duration_secs as usize * 32_000;
        let mut buffer = Vec::with_capacity(estimated_size);

        let capture_duration = Duration::from_secs(duration_secs as u64);

        // Capture for the specified duration
        let result = timeout(capture_duration, async {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(data) => {
                        buffer.extend_from_slice(&data);
                    }
                    Err(e) => {
                        warn!(error = %e, "Stream chunk error");
                        return Err(AudioError::StreamError(e));
                    }
                }
            }
            Ok(())
        })
        .await;

        match result {
            Ok(Ok(())) => {
                // Stream ended before timeout
                debug!(bytes = buffer.len(), "Stream ended early");
            }
            Ok(Err(e)) => {
                // Stream error
                return Err(e);
            }
            Err(_) => {
                // Timeout - this is expected behavior
                debug!(bytes = buffer.len(), "Capture timeout (expected)");
            }
        }

        if buffer.is_empty() {
            return Err(AudioError::EmptyStream);
        }

        debug!(bytes = buffer.len(), duration_secs, "Capture complete");
        Ok(buffer)
    }
}

impl Default for StreamCapture {
    fn default() -> Self {
        Self::new()
    }
}
