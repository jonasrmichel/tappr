// Test that mimics exactly how the main app plays audio
use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use rodio::{OutputStream, Sink, Source};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

fn main() {
    println!("Testing audio playback matching main app behavior...\n");

    // Create output stream (like PlaybackEngine::with_device)
    let (_stream, stream_handle) = OutputStream::try_default()
        .expect("Failed to open audio device");

    let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

    // Create a buffer of samples (like LoopBuffer)
    // 2 seconds of 440Hz sine wave at 48kHz stereo
    let duration_secs = 2.0;
    let num_samples = (duration_secs * SAMPLE_RATE as f32 * CHANNELS as f32) as usize;

    let samples: Vec<f32> = (0..num_samples)
        .map(|i| {
            let frame = i / CHANNELS as usize;
            let t = frame as f32 / SAMPLE_RATE as f32;
            (440.0 * 2.0 * std::f32::consts::PI * t).sin() * 0.5
        })
        .collect();

    println!("Created buffer: {} samples, {:.2}s duration", samples.len(), duration_secs);

    let max_sample = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    println!("Buffer stats: max_sample={:.4}, rms={:.4}", max_sample, rms);

    // Create looping source (like LoopingSource)
    let source = LoopingTestSource::new(samples);

    println!("\nAppending source to sink...");
    sink.append(source);

    println!("Sink empty: {}", sink.empty());
    println!("Sink paused: {}", sink.is_paused());
    println!("Sink volume: {}", sink.volume());

    println!("\nPlaying for 5 seconds (you should hear a continuous tone)...");

    // Don't use sleep_until_end since source is infinite
    // Instead, sleep like the main app does
    std::thread::sleep(Duration::from_secs(5));

    println!("\nStopping...");
    sink.clear();

    println!("Done!");
}

// Simplified looping source matching the app's LoopingSource
struct LoopingTestSource {
    samples: Arc<[f32]>,
    position: Arc<Mutex<usize>>,
}

impl LoopingTestSource {
    fn new(samples: Vec<f32>) -> Self {
        Self {
            samples: samples.into(),
            position: Arc::new(Mutex::new(0)),
        }
    }
}

impl Iterator for LoopingTestSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let mut pos = self.position.lock();
        let sample = self.samples[*pos];
        *pos = (*pos + 1) % self.samples.len();
        Some(sample)
    }
}

impl Source for LoopingTestSource {
    fn current_frame_len(&self) -> Option<usize> {
        None  // Infinite source
    }

    fn channels(&self) -> u16 {
        CHANNELS
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None  // Infinite looping
    }
}
