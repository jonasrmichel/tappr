mod app;
mod audio;
mod cli;
mod error;
mod playback;
mod radio;
mod tasks;
mod tui;

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info, Level};
use tracing_subscriber::EnvFilter;

use crate::app::{AppState, BpmMode};
use crate::audio::AudioPipeline;
use crate::cli::Args;
use crate::error::Result;
use crate::playback::PlaybackEngine;
use crate::radio::RadioService;
use crate::tasks::{Channels, Producer, ProducerConfig, ProducerEvent};
use crate::tui::TuiApp;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing (to file if TUI is enabled)
    init_tracing(args.verbose);

    info!("tappr v{} starting", env!("CARGO_PKG_VERSION"));

    // Create shared application state
    let state = AppState::new(&args);

    // Set up graceful shutdown
    let shutdown_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for Ctrl-C: {}", e);
            return;
        }
        info!("Received Ctrl-C, shutting down...");
        shutdown_state.quit();
    });

    // Run the application
    if let Err(e) = run(state, args).await {
        error!("Application error: {}", e);
        return Err(e);
    }

    info!("tappr shutdown complete");
    Ok(())
}

/// Initialize tracing subscriber
fn init_tracing(verbose: bool) {
    let level = if verbose { Level::DEBUG } else { Level::INFO };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .compact()
        .init();
}

/// Main application loop
async fn run(state: Arc<AppState>, args: Args) -> Result<()> {
    info!(
        search = ?args.search,
        region = ?args.region,
        random = args.is_random(),
        bars = args.bars,
        bpm = ?args.bpm,
        "Starting session"
    );

    // Initialize services
    let radio = RadioService::new(args.rate_limit_ms, args.cache_dir.clone());
    let audio = AudioPipeline::new(args.bpm_min, args.bpm_max);

    // Initialize playback engine
    let mut playback = PlaybackEngine::new()?;

    // Set up channels for task communication
    let channels = Channels::new();
    let (cmd_tx, cmd_rx, event_tx, mut event_rx) = channels.split();

    // Configure producer
    let bpm_mode = args
        .bpm
        .map(BpmMode::Fixed)
        .unwrap_or(BpmMode::Auto {
            min: args.bpm_min,
            max: args.bpm_max,
        });

    let producer_config = ProducerConfig {
        search: args.search.clone(),
        region: args.region.clone(),
        listen_seconds: args.listen_seconds,
        station_change_seconds: args.station_change_seconds,
        bars: args.bars,
        beats_per_bar: args.beats_per_bar(),
        bpm_mode,
    };

    // Start producer task
    let producer = Producer::new(
        producer_config,
        radio,
        audio,
        Arc::clone(&state),
        cmd_rx,
        event_tx,
    );
    tokio::spawn(async move {
        producer.run().await;
    });

    // Initialize TUI
    let mut tui = TuiApp::new(Arc::clone(&state), cmd_tx)?;

    info!("TUI started - press 'q' to quit");

    // Main event loop
    loop {
        // Handle TUI input
        let should_quit = tui.handle_input().await?;
        if should_quit || state.is_quitting() {
            break;
        }

        // Process producer events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                ProducerEvent::StationSelected(station) => {
                    tui.set_loading(station);
                }
                ProducerEvent::LoopReady(buffer, station) => {
                    let loop_info = buffer.loop_info.clone();

                    // Start or swap playback
                    if playback.is_playing() {
                        playback.queue_next(buffer);
                    } else {
                        playback.play(buffer);
                    }

                    tui.set_playing(station, loop_info);
                }
                ProducerEvent::Error(msg) => {
                    tui.set_error(msg);
                }
                ProducerEvent::Shutdown => {
                    info!("Producer shutdown");
                    break;
                }
            }
        }

        // Draw TUI
        let settings = state.settings.read().await.clone();
        tui.draw(&settings)?;

        // Small delay to prevent busy loop
        tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60 FPS
    }

    // Clean shutdown
    playback.stop();
    tui.cleanup();

    Ok(())
}
