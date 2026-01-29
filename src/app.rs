use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

use crate::cli::Args;
use crate::playback::{list_audio_devices, default_device_index, AudioDevice};

/// Station metadata
#[derive(Debug, Clone)]
pub struct StationInfo {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub country: String,
    pub place_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub stream_url: Option<String>,
    /// Station's homepage URL
    #[allow(dead_code)]
    pub website: Option<String>,
}

impl Default for StationInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::from("Unknown Station"),
            country: String::new(),
            place_name: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            stream_url: None,
            website: None,
        }
    }
}

/// Loop metadata after quantization
#[derive(Debug, Clone)]
pub struct LoopInfo {
    /// Target/output BPM (after time-stretching if applied)
    pub bpm: f32,
    /// Original detected BPM of source audio (before time-stretching)
    pub source_bpm: f32,
    /// BPM detection confidence (0.0-1.0)
    pub bpm_confidence: f32,
    /// Whether time-stretching was applied
    pub time_stretched: bool,
    /// Number of bars in the loop
    pub bars: u8,
    /// Beats per bar
    #[allow(dead_code)]
    pub beats_per_bar: u8,
    /// Duration in samples (frames)
    pub duration_samples: usize,
    /// Sample rate
    pub sample_rate: u32,
}

impl LoopInfo {
    /// Loop duration in seconds
    #[allow(dead_code)]
    pub fn duration_secs(&self) -> f32 {
        self.duration_samples as f32 / self.sample_rate as f32
    }
}

/// BPM mode selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BpmMode {
    /// Auto-detect BPM within range
    Auto { min: f32, max: f32 },
    /// Use fixed BPM
    Fixed(f32),
}

impl BpmMode {
    #[allow(dead_code)]
    pub fn is_auto(&self) -> bool {
        matches!(self, BpmMode::Auto { .. })
    }
}

/// User-configurable settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub bpm_mode: BpmMode,
    pub bars: u8,
    pub beats_per_bar: u8,
    pub listen_seconds: u32,
    #[allow(dead_code)]
    pub clip_seconds: u32,
    #[allow(dead_code)]
    pub station_change_seconds: u32,
    #[allow(dead_code)]
    pub min_rms: f32,
    #[allow(dead_code)]
    pub max_silence: f32,
    /// Selected audio output device index
    pub audio_device_index: usize,
    /// Cached list of available audio devices
    pub audio_devices: Vec<AudioDevice>,
}

impl Settings {
    pub fn from_args(args: &Args) -> Self {
        let bpm_mode = args
            .bpm
            .map(BpmMode::Fixed)
            .unwrap_or(BpmMode::Auto {
                min: args.bpm_min,
                max: args.bpm_max,
            });

        let audio_devices = list_audio_devices();
        let audio_device_index = default_device_index();

        Self {
            bpm_mode,
            bars: args.bars,
            beats_per_bar: args.beats_per_bar(),
            listen_seconds: args.listen_seconds,
            clip_seconds: args.clip_seconds,
            station_change_seconds: args.station_change_seconds,
            min_rms: args.min_rms,
            max_silence: args.max_silence,
            audio_device_index,
            audio_devices,
        }
    }

    /// Cycle bars through 1 -> 2 -> 4 -> 8 -> 16 -> 1
    pub fn cycle_bars_up(&mut self) {
        self.bars = match self.bars {
            1 => 2,
            2 => 4,
            4 => 8,
            8 => 16,
            _ => 1,
        };
    }

    /// Cycle bars through 16 -> 8 -> 4 -> 2 -> 1 -> 16
    pub fn cycle_bars_down(&mut self) {
        self.bars = match self.bars {
            16 => 8,
            8 => 4,
            4 => 2,
            2 => 1,
            _ => 16,
        };
    }

    /// Toggle between auto and fixed BPM mode
    pub fn toggle_bpm_mode(&mut self) {
        self.bpm_mode = match self.bpm_mode {
            BpmMode::Auto { .. } => BpmMode::Fixed(120.0),
            BpmMode::Fixed(_) => BpmMode::Auto { min: 70.0, max: 170.0 },
        };
    }

    /// Cycle to next audio device (refreshes device list first)
    pub fn next_audio_device(&mut self) -> bool {
        // Refresh device list in case devices changed
        self.audio_devices = list_audio_devices();

        if self.audio_devices.is_empty() {
            return false;
        }

        // Ensure current index is valid
        if self.audio_device_index >= self.audio_devices.len() {
            self.audio_device_index = 0;
        }

        let new_index = (self.audio_device_index + 1) % self.audio_devices.len();
        if new_index != self.audio_device_index {
            self.audio_device_index = new_index;
            true
        } else {
            false
        }
    }

    /// Get the current audio device name
    pub fn current_audio_device_name(&self) -> &str {
        self.audio_devices
            .get(self.audio_device_index)
            .map(|d| d.name.as_str())
            .unwrap_or("Default")
    }
}

/// Current playback state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PlaybackState {
    /// Initial state, no audio playing
    Idle,
    /// Loading a new station
    Loading { station: StationInfo },
    /// Actively playing a loop
    Playing {
        station: StationInfo,
        loop_info: LoopInfo,
    },
    /// Error state (will retry)
    Error { message: String },
}

impl Default for PlaybackState {
    fn default() -> Self {
        PlaybackState::Idle
    }
}

#[allow(dead_code)]
impl PlaybackState {
    /// Get current station if any
    pub fn station(&self) -> Option<&StationInfo> {
        match self {
            PlaybackState::Loading { station } => Some(station),
            PlaybackState::Playing { station, .. } => Some(station),
            _ => None,
        }
    }

    /// Check if actively playing
    pub fn is_playing(&self) -> bool {
        matches!(self, PlaybackState::Playing { .. })
    }

    /// Get status string for display
    pub fn status_text(&self) -> &'static str {
        match self {
            PlaybackState::Idle => "IDLE",
            PlaybackState::Loading { .. } => "LOADING",
            PlaybackState::Playing { .. } => "PLAYING",
            PlaybackState::Error { .. } => "ERROR",
        }
    }
}

/// Shared application state
pub struct AppState {
    /// User settings (modifiable via TUI)
    pub settings: RwLock<Settings>,
    /// Current playback state (watch channel for TUI updates)
    #[allow(dead_code)]
    playback_tx: watch::Sender<PlaybackState>,
    /// Receiver for playback state
    #[allow(dead_code)]
    playback_rx: watch::Receiver<PlaybackState>,
    /// Station history for display
    #[allow(dead_code)]
    pub station_history: RwLock<Vec<StationInfo>>,
    /// Shutdown flag
    pub should_quit: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(args: &Args) -> Arc<Self> {
        let settings = Settings::from_args(args);
        let (playback_tx, playback_rx) = watch::channel(PlaybackState::Idle);

        Arc::new(Self {
            settings: RwLock::new(settings),
            playback_tx,
            playback_rx,
            station_history: RwLock::new(Vec::new()),
            should_quit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Update playback state
    #[allow(dead_code)]
    pub fn set_playback_state(&self, state: PlaybackState) {
        let _ = self.playback_tx.send(state);
    }

    /// Subscribe to playback state changes
    #[allow(dead_code)]
    pub fn subscribe_playback(&self) -> watch::Receiver<PlaybackState> {
        self.playback_rx.clone()
    }

    /// Get current playback state
    #[allow(dead_code)]
    pub fn playback_state(&self) -> PlaybackState {
        self.playback_rx.borrow().clone()
    }

    /// Signal shutdown
    pub fn quit(&self) {
        self.should_quit.store(true, Ordering::SeqCst);
    }

    /// Check if shutdown requested
    pub fn is_quitting(&self) -> bool {
        self.should_quit.load(Ordering::SeqCst)
    }

    /// Add station to history
    #[allow(dead_code)]
    pub async fn add_to_history(&self, station: StationInfo) {
        let mut history = self.station_history.write().await;
        // Keep last 10 stations
        if history.len() >= 10 {
            history.remove(0);
        }
        history.push(station);
    }
}
