# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tappr** - A Rust terminal-based radio sampler that samples Radio Garden stations, quantizes audio to beats, and plays loops in real-time with a TUI world map.

## Build Commands

```bash
cargo build              # Build the project
cargo build --release    # Build with optimizations
cargo run                # Run the project
cargo run -- --search "jazz"  # Run with search query
cargo test               # Run all tests
cargo test quantize      # Run specific test module
cargo clippy             # Run linter
cargo fmt                # Format code
```

## Architecture

```
src/
├── main.rs          # Entry point, CLI parsing, main event loop
├── cli.rs           # clap derive structs for CLI arguments
├── app.rs           # AppState, Settings, PlaybackState, StationInfo
├── error.rs         # Error types (TapprError, RadioError, AudioError, etc.)
├── radio/           # Radio Garden API integration
│   ├── client.rs    # HTTP client with rate limiting
│   ├── types.rs     # API response types
│   └── cache.rs     # In-memory cache for places
├── audio/           # Audio processing pipeline
│   ├── stream.rs    # HTTP stream capture
│   ├── decode.rs    # ffmpeg subprocess for PCM decoding
│   ├── quantize.rs  # BPM detection, beat alignment
│   └── buffer.rs    # LoopBuffer, RawAudioBuffer types
├── playback/        # Audio playback
│   ├── engine.rs    # rodio playback management
│   └── source.rs    # Custom looping Source with bar-boundary swap
├── tasks/           # Background task orchestration
│   ├── producer.rs  # Station → stream → decode → quantize pipeline
│   └── channels.rs  # mpsc channel types for communication
└── tui/             # Terminal UI
    ├── app.rs       # TUI event loop, input handling
    └── widgets/     # Settings, NowPlaying, WorldMap panels
```

## Key Components

- **RadioService** (`src/radio/mod.rs`): Fetches stations from Radio Garden API with caching
- **AudioPipeline** (`src/audio/mod.rs`): Orchestrates stream capture → decode → quantize
- **PlaybackEngine** (`src/playback/engine.rs`): rodio-based seamless loop playback
- **Producer** (`src/tasks/producer.rs`): Background task for continuous station cycling
- **TuiApp** (`src/tui/app.rs`): Crossterm-based TUI with ratatui widgets

## Data Flow

1. Producer task fetches station from Radio Garden API
2. Captures N seconds of HTTP stream
3. Decodes via ffmpeg subprocess to 48kHz stereo PCM
4. Detects BPM via autocorrelation, aligns to beat grid
5. Sends LoopBuffer to main loop via mpsc channel
6. PlaybackEngine plays loop, swaps at bar boundary
7. TUI renders state at 60 FPS via watch channel

## External Dependencies

- **ffmpeg**: Required for audio decoding (must be in PATH)
- **Radio Garden API**: `https://radio.garden/api` (rate limited)
