use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{debug, info, instrument, warn};

use crate::audio::LoopBuffer;
use crate::error::PlaybackError;

use super::source::OneShotSource;

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,
    #[allow(dead_code)]
    pub index: usize,
}

/// Get list of available audio output devices
pub fn list_audio_devices() -> Vec<AudioDevice> {
    let host = rodio::cpal::default_host();
    let mut devices = Vec::new();

    match host.output_devices() {
        Ok(output_devices) => {
            for (index, device) in output_devices.enumerate() {
                let name = device.name().unwrap_or_else(|_| format!("Device {}", index));
                devices.push(AudioDevice { name, index });
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to enumerate audio devices");
        }
    }

    if devices.is_empty() {
        devices.push(AudioDevice {
            name: "Default".to_string(),
            index: 0,
        });
    }

    devices
}

/// Get the default device index
pub fn default_device_index() -> usize {
    let host = rodio::cpal::default_host();
    if let Some(default) = host.default_output_device() {
        if let Ok(default_name) = default.name() {
            if let Ok(devices) = host.output_devices() {
                for (index, device) in devices.enumerate() {
                    if let Ok(name) = device.name() {
                        if name == default_name {
                            return index;
                        }
                    }
                }
            }
        }
    }
    0
}

/// Playback engine managing audio output
pub struct PlaybackEngine {
    /// Keep the stream alive (dropping it stops audio)
    _stream: OutputStream,
    /// Handle for creating sinks
    _stream_handle: OutputStreamHandle,
    /// Audio sink for playback control
    sink: Sink,
    /// Current device index
    #[allow(dead_code)]
    device_index: usize,
}

impl PlaybackEngine {
    /// Create a new playback engine with the default device
    #[allow(dead_code)]
    #[instrument]
    pub fn new() -> Result<Self, PlaybackError> {
        Self::with_device(None)
    }

    /// Create a new playback engine with a specific device
    #[instrument]
    pub fn with_device(device_index: Option<usize>) -> Result<Self, PlaybackError> {
        info!("Initializing audio output");

        let host = rodio::cpal::default_host();

        let (stream, stream_handle, actual_index) = if let Some(index) = device_index {
            // Try to get the specific device
            if let Ok(mut devices) = host.output_devices() {
                if let Some(device) = devices.nth(index) {
                    let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
                    info!(device = %name, index, "Using selected audio device");

                    match OutputStream::try_from_device(&device) {
                        Ok((stream, handle)) => (stream, handle, index),
                        Err(e) => {
                            warn!(error = %e, "Failed to open selected device, using default");
                            let (stream, handle) = OutputStream::try_default()
                                .map_err(|e| PlaybackError::Device(format!("Failed to open audio device: {}", e)))?;
                            (stream, handle, default_device_index())
                        }
                    }
                } else {
                    warn!(index, "Device index out of range, using default");
                    let (stream, handle) = OutputStream::try_default()
                        .map_err(|e| PlaybackError::Device(format!("Failed to open audio device: {}", e)))?;
                    (stream, handle, default_device_index())
                }
            } else {
                let (stream, handle) = OutputStream::try_default()
                    .map_err(|e| PlaybackError::Device(format!("Failed to open audio device: {}", e)))?;
                (stream, handle, default_device_index())
            }
        } else {
            let (stream, handle) = OutputStream::try_default()
                .map_err(|e| PlaybackError::Device(format!("Failed to open audio device: {}", e)))?;
            (stream, handle, default_device_index())
        };

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| PlaybackError::Device(format!("Failed to create audio sink: {}", e)))?;

        debug!("Audio output initialized");

        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
            device_index: actual_index,
        })
    }

    /// Get current device index
    #[allow(dead_code)]
    pub fn device_index(&self) -> usize {
        self.device_index
    }

    /// Start playing an audio buffer (plays once, no looping)
    #[instrument(skip(self, buffer))]
    pub fn play(&mut self, buffer: LoopBuffer) {
        // Analyze buffer for diagnostics
        let sample_count = buffer.samples.len();
        let max_sample = buffer.samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let rms = (buffer.samples.iter().map(|s| s * s).sum::<f32>() / sample_count as f32).sqrt();

        info!(
            bpm = buffer.loop_info.bpm,
            bars = buffer.loop_info.bars,
            duration_secs = buffer.duration_secs(),
            sample_count,
            max_sample,
            rms,
            "Starting playback"
        );

        // Create the one-shot source
        let source = OneShotSource::new(buffer);

        // Clear any existing playback
        self.sink.clear();

        // Start the new source
        self.sink.append(source);

        // Ensure sink is playing (clear() may pause it)
        self.sink.play();

        // Log sink state
        info!(
            sink_empty = self.sink.empty(),
            sink_paused = self.sink.is_paused(),
            sink_volume = self.sink.volume(),
            "Playback started"
        );
    }

    /// Append a buffer to the playback queue (for gapless playback)
    #[instrument(skip(self, buffer))]
    pub fn append(&mut self, buffer: LoopBuffer) {
        info!(
            bpm = buffer.loop_info.bpm,
            bars = buffer.loop_info.bars,
            duration_secs = buffer.duration_secs(),
            "Queueing next clip"
        );

        let source = OneShotSource::new(buffer);
        self.sink.append(source);

        // Ensure sink is playing
        self.sink.play();
    }

    /// Check if audio is currently playing
    #[allow(dead_code)]
    pub fn is_playing(&self) -> bool {
        !self.sink.empty() && !self.sink.is_paused()
    }

    /// Check if playback has finished (sink is empty)
    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }

    /// Get the number of sources queued in the sink
    #[allow(dead_code)]
    pub fn queue_len(&self) -> usize {
        self.sink.len()
    }

    /// Pause playback
    #[allow(dead_code)]
    pub fn pause(&self) {
        debug!("Pausing playback");
        self.sink.pause();
    }

    /// Resume playback
    #[allow(dead_code)]
    pub fn resume(&self) {
        debug!("Resuming playback");
        self.sink.play();
    }

    /// Skip to the next queued source
    #[instrument(skip(self))]
    pub fn skip_one(&self) {
        info!("Skipping to next clip");
        self.sink.skip_one();
    }

    /// Stop playback
    #[instrument(skip(self))]
    pub fn stop(&mut self) {
        info!("Stopping playback");
        self.sink.clear();
    }

    /// Set playback volume (0.0 to 1.0)
    #[allow(dead_code)]
    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        debug!(volume, "Setting volume");
        self.sink.set_volume(volume);
    }

    /// Get current volume
    #[allow(dead_code)]
    pub fn volume(&self) -> f32 {
        self.sink.volume()
    }
}

impl Drop for PlaybackEngine {
    fn drop(&mut self) {
        debug!("Dropping playback engine");
        self.stop();
    }
}
