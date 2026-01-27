use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

use crate::app::{AppState, BpmMode};
use crate::audio::AudioPipeline;
use crate::radio::RadioService;

use super::channels::{ProducerCommand, ProducerEvent};

/// Number of parallel fetch workers
const NUM_WORKERS: usize = 3;

/// Producer task configuration
#[derive(Clone)]
pub struct ProducerConfig {
    pub search: Option<String>,
    pub region: Option<String>,
    pub listen_seconds: u32,
    #[allow(dead_code)]
    pub station_change_seconds: u32,
    pub bars: u8,
    pub beats_per_bar: u8,
    pub bpm_mode: BpmMode,
}

/// Producer coordinator that spawns parallel fetch workers
pub struct Producer {
    config: ProducerConfig,
    state: Arc<AppState>,
    cmd_rx: mpsc::Receiver<ProducerCommand>,
    event_tx: mpsc::Sender<ProducerEvent>,
    rate_limit_ms: u64,
    cache_dir: Option<std::path::PathBuf>,
    bpm_min: f32,
    bpm_max: f32,
}

impl Producer {
    #[allow(dead_code)]
    pub fn new(
        config: ProducerConfig,
        radio: RadioService,
        audio: AudioPipeline,
        state: Arc<AppState>,
        cmd_rx: mpsc::Receiver<ProducerCommand>,
        event_tx: mpsc::Sender<ProducerEvent>,
    ) -> Self {
        // Extract config from radio and audio for worker creation
        // We'll recreate these per-worker
        let _ = radio; // Consume but don't use - we'll create new ones per worker
        let _ = audio;

        Self {
            config,
            state,
            cmd_rx,
            event_tx,
            rate_limit_ms: 500, // Default rate limit
            cache_dir: None,
            bpm_min: 70.0,
            bpm_max: 170.0,
        }
    }

    /// Create with explicit parameters for workers
    pub fn with_params(
        config: ProducerConfig,
        state: Arc<AppState>,
        cmd_rx: mpsc::Receiver<ProducerCommand>,
        event_tx: mpsc::Sender<ProducerEvent>,
        rate_limit_ms: u64,
        cache_dir: Option<std::path::PathBuf>,
        bpm_min: f32,
        bpm_max: f32,
    ) -> Self {
        Self {
            config,
            state,
            cmd_rx,
            event_tx,
            rate_limit_ms,
            cache_dir,
            bpm_min,
            bpm_max,
        }
    }

    /// Run the producer with parallel workers
    #[instrument(skip(self), name = "producer")]
    pub async fn run(mut self) {
        info!(workers = NUM_WORKERS, "Producer starting with parallel workers");

        // Channel for workers to send completed clips
        let (clip_tx, mut clip_rx) = mpsc::channel::<(crate::audio::LoopBuffer, crate::app::StationInfo)>(NUM_WORKERS * 2);

        // Spawn worker tasks
        for worker_id in 0..NUM_WORKERS {
            let config = self.config.clone();
            let state = Arc::clone(&self.state);
            let clip_tx = clip_tx.clone();
            let event_tx = self.event_tx.clone();
            let rate_limit_ms = self.rate_limit_ms;
            let cache_dir = self.cache_dir.clone();
            let bpm_min = self.bpm_min;
            let bpm_max = self.bpm_max;

            tokio::spawn(async move {
                run_worker(
                    worker_id,
                    config,
                    state,
                    clip_tx,
                    event_tx,
                    rate_limit_ms,
                    cache_dir,
                    bpm_min,
                    bpm_max,
                ).await;
            });
        }

        // Drop our copy of clip_tx so channel closes when all workers finish
        drop(clip_tx);

        // Main coordinator loop
        loop {
            tokio::select! {
                // Receive completed clips from workers and forward to main
                Some((buffer, station)) = clip_rx.recv() => {
                    let _ = self.event_tx.send(ProducerEvent::LoopReady(buffer, station)).await;
                }

                // Handle commands from TUI
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        ProducerCommand::NextStation => {
                            debug!("Received NextStation command");
                            // Workers are already continuously fetching
                        }
                        ProducerCommand::AudioDeviceChanged(device_index) => {
                            debug!(device_index, "Received AudioDeviceChanged command");
                            let _ = self.event_tx.send(ProducerEvent::AudioDeviceChanged(device_index)).await;
                        }
                        ProducerCommand::Quit => {
                            info!("Received Quit command");
                            self.state.quit(); // Signal workers to stop
                            break;
                        }
                    }
                }

                // Check for shutdown
                else => {
                    if self.state.is_quitting() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }

        info!("Producer shutting down");
        let _ = self.event_tx.send(ProducerEvent::Shutdown).await;
    }
}

/// Worker task that continuously fetches and processes stations
async fn run_worker(
    worker_id: usize,
    config: ProducerConfig,
    state: Arc<AppState>,
    clip_tx: mpsc::Sender<(crate::audio::LoopBuffer, crate::app::StationInfo)>,
    event_tx: mpsc::Sender<ProducerEvent>,
    rate_limit_ms: u64,
    cache_dir: Option<std::path::PathBuf>,
    bpm_min: f32,
    bpm_max: f32,
) {
    info!(worker_id, "Worker starting");

    // Each worker gets its own radio client and audio pipeline
    let radio = RadioService::new(rate_limit_ms, cache_dir);
    let audio = AudioPipeline::new(bpm_min, bpm_max);

    // Stagger worker start times to avoid thundering herd
    tokio::time::sleep(Duration::from_millis(worker_id as u64 * 500)).await;

    loop {
        if state.is_quitting() {
            break;
        }

        // Fetch and process a station
        match fetch_and_process(
            worker_id,
            &config,
            &radio,
            &audio,
            &event_tx,
        ).await {
            Ok((buffer, station)) => {
                // Send completed clip to coordinator
                if clip_tx.send((buffer, station)).await.is_err() {
                    // Channel closed, exit
                    break;
                }
            }
            Err(e) => {
                warn!(worker_id, error = %e, "Worker failed to process station");
                // Wait before retrying on error
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        // Small delay between fetches
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    info!(worker_id, "Worker stopping");
}

/// Fetch a station and process its audio
async fn fetch_and_process(
    worker_id: usize,
    config: &ProducerConfig,
    radio: &RadioService,
    audio: &AudioPipeline,
    event_tx: &mpsc::Sender<ProducerEvent>,
) -> Result<(crate::audio::LoopBuffer, crate::app::StationInfo), Box<dyn std::error::Error + Send + Sync>> {
    // Get next station
    let station = radio
        .next_station(config.search.as_deref(), config.region.as_deref())
        .await?;

    info!(
        worker_id,
        name = %station.name,
        country = %station.country,
        place = %station.place_name,
        "Worker selected station"
    );

    // Notify station selected
    let _ = event_tx
        .send(ProducerEvent::StationSelected(station.clone()))
        .await;

    // Process audio
    let buffer = audio
        .process_station(
            &station,
            config.listen_seconds,
            config.bpm_mode,
            config.bars,
            config.beats_per_bar,
        )
        .await?;

    info!(
        worker_id,
        bpm = buffer.loop_info.bpm,
        confidence = format!("{:.0}%", buffer.loop_info.bpm_confidence * 100.0),
        duration_secs = buffer.duration_secs(),
        "Worker clip ready"
    );

    Ok((buffer, station))
}
