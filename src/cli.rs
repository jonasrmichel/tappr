use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "tappr")]
#[command(about = "Ride the beat of the world's airwaves")]
#[command(version)]
pub struct Args {
    // Station selection
    /// Search for stations by query
    #[arg(long)]
    pub search: Option<String>,

    /// Filter by region/country
    #[arg(long)]
    pub region: Option<String>,

    /// Use random station selection (default if no search/region)
    #[arg(long)]
    pub random: bool,

    /// Seed for reproducible random selection
    #[arg(long)]
    pub seed: Option<u64>,

    // Timing
    /// Duration to capture from stream (seconds)
    #[arg(long, default_value = "20")]
    pub listen_seconds: u32,

    /// Duration of captured clip (seconds)
    #[arg(long, default_value = "4")]
    pub clip_seconds: u32,

    /// Duration before changing stations (seconds)
    #[arg(long, default_value = "12")]
    pub station_change_seconds: u32,

    /// Number of bars per clip (more bars = longer clip)
    #[arg(long, default_value = "8", value_parser = clap::value_parser!(u8).range(1..=16))]
    pub bars: u8,

    /// Time signature (beats per bar)
    #[arg(long, default_value = "4/4")]
    pub meter: String,

    // BPM
    /// Fixed BPM (disables auto-detection)
    #[arg(long)]
    pub bpm: Option<f32>,

    /// Minimum BPM for auto-detection
    #[arg(long, default_value = "70")]
    pub bpm_min: f32,

    /// Maximum BPM for auto-detection
    #[arg(long, default_value = "170")]
    pub bpm_max: f32,

    // Heuristics
    /// Minimum RMS threshold for audio
    #[arg(long, default_value = "0.01")]
    pub min_rms: f32,

    /// Maximum silence duration (seconds)
    #[arg(long, default_value = "2.0")]
    pub max_silence: f32,

    /// Rate limit between API requests (ms)
    #[arg(long, default_value = "500")]
    pub rate_limit_ms: u64,

    // Debug
    /// Custom cache directory
    #[arg(long)]
    pub cache_dir: Option<std::path::PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

impl Args {
    /// Parse meter string (e.g., "4/4") into beats per bar
    pub fn beats_per_bar(&self) -> u8 {
        self.meter
            .split('/')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4)
    }

    /// Check if using default random selection
    pub fn is_random(&self) -> bool {
        self.random || (self.search.is_none() && self.region.is_none())
    }
}
