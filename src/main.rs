mod app;
mod cli;
mod error;
mod radio;

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info, Level};
use tracing_subscriber::EnvFilter;

use crate::app::AppState;
use crate::cli::Args;
use crate::error::Result;
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

    // TODO: Phase 3 - Initialize audio pipeline
    // TODO: Phase 4 - Initialize playback engine
    // TODO: Phase 5 - Start producer task
    // TODO: Phase 6 - Start TUI

    // For now, just wait for shutdown signal
    info!("Waiting for shutdown signal (Ctrl-C)...");
    while !state.is_quitting() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    Ok(())
}
