use std::sync::Arc;

use parking_lot::Mutex;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{debug, error, info, instrument};

use crate::audio::LoopBuffer;
use crate::error::PlaybackError;

use super::source::{LoopControl, LoopingSource};

/// Playback engine managing audio output
pub struct PlaybackEngine {
    /// Keep the stream alive (dropping it stops audio)
    _stream: OutputStream,
    /// Handle for creating sinks
    _stream_handle: OutputStreamHandle,
    /// Audio sink for playback control
    sink: Sink,
    /// Control handle for the current looping source
    loop_control: Option<Arc<Mutex<LoopControl>>>,
}

impl PlaybackEngine {
    /// Create a new playback engine
    #[instrument]
    pub fn new() -> Result<Self, PlaybackError> {
        info!("Initializing audio output");

        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| PlaybackError::Device(format!("Failed to open audio device: {}", e)))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| PlaybackError::Device(format!("Failed to create audio sink: {}", e)))?;

        debug!("Audio output initialized");

        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
            loop_control: None,
        })
    }

    /// Start playing a loop buffer
    #[instrument(skip(self, buffer))]
    pub fn play(&mut self, buffer: LoopBuffer) {
        info!(
            bpm = buffer.loop_info.bpm,
            bars = buffer.loop_info.bars,
            duration_secs = buffer.duration_secs(),
            "Starting playback"
        );

        // Create the looping source
        let (source, control) = LoopingSource::new(buffer);

        // Clear any existing playback
        self.sink.clear();

        // Start the new source
        self.sink.append(source);
        self.loop_control = Some(control);

        debug!("Playback started");
    }

    /// Queue a new buffer to play at the next bar boundary
    #[instrument(skip(self, buffer))]
    pub fn queue_next(&mut self, buffer: LoopBuffer) {
        if let Some(control) = &self.loop_control {
            info!(
                bpm = buffer.loop_info.bpm,
                bars = buffer.loop_info.bars,
                "Queueing next loop"
            );

            let mut ctrl = control.lock();
            ctrl.queue_swap(buffer);

            debug!("Next loop queued");
        } else {
            // No current playback, just start playing
            self.play(buffer);
        }
    }

    /// Check if audio is currently playing
    pub fn is_playing(&self) -> bool {
        !self.sink.empty() && !self.sink.is_paused()
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

    /// Stop playback
    #[instrument(skip(self))]
    pub fn stop(&mut self) {
        info!("Stopping playback");
        self.sink.clear();
        self.loop_control = None;
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
