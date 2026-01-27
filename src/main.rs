mod app;
mod audio;
mod cli;
mod error;
mod playback;
mod radio;

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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
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

    // Initialize Radio Garden service
    let radio = RadioService::new(args.rate_limit_ms, args.cache_dir.clone());

    // Get initial station
    let station = radio
        .next_station(args.search.as_deref(), args.region.as_deref())
        .await?;

    info!(
        name = %station.name,
        country = %station.country,
        place = %station.place_name,
        lat = station.latitude,
        lon = station.longitude,
        stream_url = ?station.stream_url,
        "Found station"
    );

    // Initialize audio pipeline
    let audio_pipeline = AudioPipeline::new(args.bpm_min, args.bpm_max);

    // Determine BPM mode
    let bpm_mode = args
        .bpm
        .map(BpmMode::Fixed)
        .unwrap_or(BpmMode::Auto {
            min: args.bpm_min,
            max: args.bpm_max,
        });

    // Process the station (capture, decode, quantize)
    info!("Processing audio...");
    let loop_buffer = audio_pipeline
        .process_station(
            &station,
            args.listen_seconds,
            bpm_mode,
            args.bars,
            args.beats_per_bar(),
        )
        .await?;

    info!(
        bpm = loop_buffer.loop_info.bpm,
        confidence = format!("{:.0}%", loop_buffer.loop_info.bpm_confidence * 100.0),
        bars = loop_buffer.loop_info.bars,
        duration_secs = loop_buffer.duration_secs(),
        samples = loop_buffer.samples.len(),
        "Loop ready"
    );

    // Initialize playback engine
    let mut playback = PlaybackEngine::new()?;

    // Start playing the loop
    playback.play(loop_buffer);
    info!("Playback started - press Ctrl-C to stop");

    // TODO: Phase 5 - Start producer task
    // TODO: Phase 6 - Start TUI

    // Wait for shutdown signal
    while !state.is_quitting() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Clean shutdown
    playback.stop();

    Ok(())
}
