use thiserror::Error;

/// Main application error type
#[derive(Error, Debug)]
pub enum TapprError {
    #[error("Radio Garden API error: {0}")]
    Radio(#[from] RadioError),

    #[error("Audio processing error: {0}")]
    Audio(#[from] AudioError),

    #[error("Playback error: {0}")]
    Playback(#[from] PlaybackError),

    #[error("TUI error: {0}")]
    Tui(#[from] TuiError),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Radio Garden API errors
#[derive(Error, Debug)]
pub enum RadioError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP status {0}")]
    HttpStatus(reqwest::StatusCode),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("No stations found")]
    NoStationsFound,

    #[error("Rate limited, retry after {0}s")]
    RateLimited(u32),

    #[error("Invalid station ID: {0}")]
    InvalidStation(String),

    #[error("Stream URL not found for station")]
    NoStreamUrl,
}

/// Audio processing errors
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Stream HTTP error: {0}")]
    StreamHttpError(reqwest::StatusCode),

    #[error("Stream error: {0}")]
    StreamError(#[source] reqwest::Error),

    #[error("Empty stream - no audio data received")]
    EmptyStream,

    #[error("ffmpeg not found - please install ffmpeg")]
    FfmpegNotFound,

    #[error("ffmpeg failed with status {0}")]
    FfmpegFailed(std::process::ExitStatus),

    #[error("ffmpeg error: {0}")]
    FfmpegError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Quantization failed: {0}")]
    QuantizationFailed(String),

    #[error("Audio too short for quantization (need at least {0}s)")]
    AudioTooShort(f32),

    #[error("Invalid sample rate: {0}")]
    InvalidSampleRate(u32),

    #[error("Decode error: {0}")]
    DecodeError(String),
}

/// Playback errors
#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("Audio stream error: {0}")]
    Stream(String),

    #[error("Audio device error: {0}")]
    Device(String),

    #[error("No audio device available")]
    NoDevice,

    #[error("Playback failed: {0}")]
    PlaybackFailed(String),
}

/// TUI errors
#[derive(Error, Debug)]
pub enum TuiError {
    #[error("Terminal IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Crossterm error: {0}")]
    Crossterm(String),

    #[error("Terminal initialization failed")]
    InitFailed,
}

/// Result type alias for tappr operations
pub type Result<T> = std::result::Result<T, TapprError>;

impl TapprError {
    /// Check if this error is recoverable (should retry)
    pub fn is_recoverable(&self) -> bool {
        match self {
            TapprError::Radio(RadioError::Http(_)) => true,
            TapprError::Radio(RadioError::RateLimited(_)) => true,
            TapprError::Audio(AudioError::StreamError(_)) => true,
            TapprError::Audio(AudioError::EmptyStream) => true,
            _ => false,
        }
    }
}
