use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, instrument, warn};

use crate::app::{AppState, BpmMode};
use crate::audio::AudioPipeline;
use crate::radio::RadioService;

use super::channels::{ProducerCommand, ProducerEvent};

/// Producer task configuration
#[derive(Clone)]
pub struct ProducerConfig {
    pub search: Option<String>,
    pub region: Option<String>,
    pub listen_seconds: u32,
    pub station_change_seconds: u32,
    pub bars: u8,
    pub beats_per_bar: u8,
    pub bpm_mode: BpmMode,
}

/// Producer task that continuously fetches and processes stations
pub struct Producer {
    config: ProducerConfig,
    radio: RadioService,
    audio: AudioPipeline,
    state: Arc<AppState>,
    cmd_rx: mpsc::Receiver<ProducerCommand>,
    event_tx: mpsc::Sender<ProducerEvent>,
}

impl Producer {
    pub fn new(
        config: ProducerConfig,
        radio: RadioService,
        audio: AudioPipeline,
        state: Arc<AppState>,
        cmd_rx: mpsc::Receiver<ProducerCommand>,
        event_tx: mpsc::Sender<ProducerEvent>,
    ) -> Self {
        Self {
            config,
            radio,
            audio,
            state,
            cmd_rx,
            event_tx,
        }
    }

    /// Run the producer task
    #[instrument(skip(self), name = "producer")]
    pub async fn run(mut self) {
        info!("Producer starting");

        // Timer for automatic station changes
        let mut change_timer = interval(Duration::from_secs(self.config.station_change_seconds as u64));
        change_timer.tick().await; // Skip first immediate tick

        // Process first station immediately
        self.process_next_station().await;

        loop {
            tokio::select! {
                // Handle commands from TUI
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        ProducerCommand::NextStation => {
                            debug!("Received NextStation command");
                            change_timer.reset(); // Reset timer
                            self.process_next_station().await;
                        }
                        ProducerCommand::Quit => {
                            info!("Received Quit command");
                            break;
                        }
                    }
                }

                // Automatic station change on timer
                _ = change_timer.tick() => {
                    if !self.state.is_quitting() {
                        debug!("Station change timer fired");
                        self.process_next_station().await;
                    }
                }

                // Check for shutdown
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.state.is_quitting() {
                        break;
                    }
                }
            }
        }

        info!("Producer shutting down");
        let _ = self.event_tx.send(ProducerEvent::Shutdown).await;
    }

    /// Fetch and process the next station
    async fn process_next_station(&mut self) {
        // Get next station
        let station = match self
            .radio
            .next_station(self.config.search.as_deref(), self.config.region.as_deref())
            .await
        {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Failed to get station");
                let _ = self
                    .event_tx
                    .send(ProducerEvent::Error(format!("Station error: {}", e)))
                    .await;
                return;
            }
        };

        info!(
            name = %station.name,
            country = %station.country,
            place = %station.place_name,
            "Selected station"
        );

        // Notify station selected
        let _ = self
            .event_tx
            .send(ProducerEvent::StationSelected(station.clone()))
            .await;

        // Process audio
        match self
            .audio
            .process_station(
                &station,
                self.config.listen_seconds,
                self.config.bpm_mode,
                self.config.bars,
                self.config.beats_per_bar,
            )
            .await
        {
            Ok(buffer) => {
                info!(
                    bpm = buffer.loop_info.bpm,
                    confidence = format!("{:.0}%", buffer.loop_info.bpm_confidence * 100.0),
                    "Loop ready"
                );

                let _ = self
                    .event_tx
                    .send(ProducerEvent::LoopReady(buffer, station))
                    .await;
            }
            Err(e) => {
                warn!(error = %e, station = %station.name, "Failed to process audio");
                let _ = self
                    .event_tx
                    .send(ProducerEvent::Error(format!(
                        "Audio error for {}: {}",
                        station.name, e
                    )))
                    .await;
            }
        }
    }
}
