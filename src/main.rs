mod app;
mod audio;
mod cli;
mod error;
mod playback;
mod radio;
mod tasks;
mod tui;

use std::fs::File;
use std::io;
use std::panic;
use std::sync::Arc;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use tracing::{error, info, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

use crate::app::{AppState, BpmMode};
use crate::cli::Args;
use crate::error::Result;
use crate::playback::PlaybackEngine;
use crate::tasks::{Channels, Producer, ProducerConfig, ProducerEvent};
use crate::tui::TuiApp;

/// Restore terminal state (used for panic hook and cleanup)
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook to restore terminal
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

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
    let result = run(state, args).await;

    // Always restore terminal on exit
    restore_terminal();

    if let Err(ref e) = result {
        error!("Application error: {}", e);
    } else {
        info!("tappr shutdown complete");
    }

    result
}

/// Initialize tracing subscriber (logs to file to avoid TUI interference)
fn init_tracing(verbose: bool) {
    let level = if verbose { Level::DEBUG } else { Level::INFO };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    // Log to file to avoid interfering with TUI
    let log_file = File::create("/tmp/tappr.log").expect("Failed to create log file");

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_writer(log_file.with_max_level(Level::TRACE))
        .with_ansi(false)
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

    // Initialize playback engine with default audio device
    let initial_device = {
        let settings = state.settings.read().await;
        settings.audio_device_index
    };
    let mut playback = PlaybackEngine::with_device(Some(initial_device))?;

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

    // Start producer task with parallel workers
    let producer = Producer::with_params(
        producer_config,
        Arc::clone(&state),
        cmd_rx,
        event_tx,
        args.rate_limit_ms,
        args.cache_dir.clone(),
        args.bpm_min,
        args.bpm_max,
    );
    tokio::spawn(async move {
        producer.run().await;
    });

    // Initialize TUI
    let mut tui = TuiApp::new(Arc::clone(&state), cmd_tx)?;

    info!("TUI started - press 'q' to quit");

    // Track when the current clip should end (time-based sync for accurate TUI updates)
    let mut current_clip_end: Option<std::time::Instant> = None;

    // Main event loop
    loop {
        // Handle TUI input
        let should_quit = tui.handle_input().await?;
        if should_quit || state.is_quitting() {
            break;
        }

        // Time-based TUI sync: advance when current clip should have ended
        // This is more accurate than sink.len() which reports late
        let now = std::time::Instant::now();
        if let Some(end_time) = current_clip_end {
            if now >= end_time && tui.queue_len() > 0 {
                // Current clip should have ended, advance TUI
                if let Some(duration) = tui.advance_queue() {
                    // Set end time for the new clip
                    current_clip_end = Some(now + std::time::Duration::from_secs_f32(duration));
                } else {
                    current_clip_end = None;
                }
            }
        }

        // Process producer events (may add more clips to the sink)
        while let Ok(event) = event_rx.try_recv() {
            match event {
                ProducerEvent::StationSelected(station) => {
                    tui.set_loading(station);
                }
                ProducerEvent::LoopReady(buffer, station) => {
                    let loop_info = buffer.loop_info.clone();
                    let duration_secs = loop_info.duration_samples as f32 / loop_info.sample_rate as f32;

                    // Append to queue for gapless playback
                    // First clip uses play(), subsequent clips use append()
                    if playback.is_finished() {
                        // This clip will play immediately
                        playback.play(buffer);
                        tui.set_now_playing(station, loop_info);
                        // Set end time for this clip
                        current_clip_end = Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(duration_secs));
                    } else {
                        // This clip is queued for later
                        playback.append(buffer);
                        tui.add_to_queue(station, loop_info);
                    }
                }
                ProducerEvent::Error(msg) => {
                    tui.set_error(msg);
                }
                ProducerEvent::SkipCurrent => {
                    info!("Skipping current station");
                    // Skip current clip in playback and immediately advance TUI
                    playback.skip_one();
                    if let Some(duration) = tui.advance_queue() {
                        current_clip_end = Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(duration));
                    } else {
                        current_clip_end = None;
                    }
                }
                ProducerEvent::AudioDeviceChanged(device_index) => {
                    info!(device_index, "Switching audio device");
                    // Stop current playback
                    playback.stop();
                    current_clip_end = None;
                    // Recreate playback engine with new device
                    match PlaybackEngine::with_device(Some(device_index)) {
                        Ok(new_playback) => {
                            playback = new_playback;
                            info!("Audio device switched successfully");
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to switch audio device");
                            tui.set_error(format!("Device switch failed: {}", e));
                        }
                    }
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
