<p align="center">
  <img src="assets/mascot.png" alt="tappr mascot" width="150" height="150">
</p>

<h1 align="center">tappr</h1>

<p align="center">
  Ride the beat of the world's airwaves
</p>

<p align="center">
  A terminal-based radio sampler that flies around the world, capturing and beat-matching live radio streams.
</p>

## Features

- **Live Radio Sampling**: Continuously samples stations from [Radio Garden](https://radio.garden)
- **BPM Detection**: Automatically detects tempo and quantizes audio to beat-aligned loops
- **World Map**: Terminal UI displays current station location on a world map
- **Seamless Transitions**: Swaps loops at bar boundaries for smooth playback
- **Interactive Controls**: Navigate stations, adjust BPM mode, and change loop length in real-time

## Requirements

- [Rust](https://rustup.rs/) 1.70+
- [ffmpeg](https://ffmpeg.org/) (for audio decoding)
- Audio output device

## Installation

```bash
# Clone the repository
git clone https://github.com/jonasrmichel/tappr.git
cd tappr

# Build and install
cargo install --path .
```

## Usage

```bash
# Start with random stations
tappr

# Search for jazz stations
tappr --search "jazz"

# Filter by country
tappr --region "Brazil"

# Fixed BPM mode (120 BPM)
tappr --bpm 120

# Custom timing
tappr --listen-seconds 15 --station-change-seconds 20 --bars 4
```

## Keyboard Controls

| Key | Action |
|-----|--------|
| `q` | Quit |
| `n` | Skip to next station |
| `b` | Toggle BPM mode (auto/fixed) |
| `+`/`-` | Increase/decrease bars (1/2/4) |

## CLI Options

```
tappr [OPTIONS]

Station Selection:
  --search <query>       Search for stations by name
  --region <country>     Filter by country/region
  --random               Use random station selection (default)
  --seed <u64>           Seed for reproducible randomness

Timing:
  --listen-seconds <n>   Duration to listen before capturing (default: 10)
  --clip-seconds <n>     Duration of captured clip (default: 4)
  --station-change-seconds <n>  Time before changing stations (default: 12)
  --bars <1|2|4>         Number of bars per loop (default: 2)
  --meter <n/4>          Time signature (default: 4/4)

BPM:
  --bpm <n>              Fixed BPM (disables auto-detection)
  --bpm-min <n>          Minimum BPM for detection (default: 70)
  --bpm-max <n>          Maximum BPM for detection (default: 170)

Debug:
  --cache-dir <path>     Custom cache directory
  --verbose              Enable debug logging
```

## How It Works

1. **Station Selection**: Fetches station metadata from Radio Garden API
2. **Stream Capture**: Records a short segment of the live audio stream
3. **Decode**: Converts to PCM using ffmpeg (supports HLS, AAC, MP3, etc.)
4. **BPM Detection**: Analyzes tempo using energy envelope and autocorrelation
5. **Quantization**: Aligns audio to beat grid and snaps length to bars
6. **Playback**: Loops seamlessly, swapping to new clips at bar boundaries

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Main Thread                       │
│   AppState ◄──watch──► TUI (render @ 60 FPS)        │
└───────────────────────────────────────────────────────┘
                          │
┌─────────────────────────┼────────────────────────────┐
│                   Tokio Runtime                       │
│   Producer ──mpsc──► Main ──► Playback Engine        │
│   (fetch/decode/quantize)      (rodio loop)          │
└───────────────────────────────────────────────────────┘
```

## License

MIT
