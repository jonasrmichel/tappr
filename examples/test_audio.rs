// Test audio playback with device selection
use std::time::Duration;
use rodio::{OutputStream, Sink, Source};
use rodio::cpal::traits::{HostTrait, DeviceTrait};

fn main() {
    println!("Testing audio playback...\n");

    // List devices
    let host = rodio::cpal::default_host();
    println!("Available output devices:");
    let devices: Vec<_> = host.output_devices()
        .map(|d| d.collect())
        .unwrap_or_default();

    for (i, device) in devices.iter().enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = host.default_output_device()
            .and_then(|d| d.name().ok())
            .map(|n| n == name)
            .unwrap_or(false);
        println!("  {}: {}{}", i, name, if is_default { " (default)" } else { "" });
    }

    // Get device index from args or use default
    let device_idx: Option<usize> = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok());

    println!();

    let (_stream, stream_handle) = if let Some(idx) = device_idx {
        if idx < devices.len() {
            let device = &devices[idx];
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            println!("Using device {}: {}", idx, name);
            OutputStream::try_from_device(device)
                .expect("Failed to open selected device")
        } else {
            println!("Device index {} out of range, using default", idx);
            OutputStream::try_default().expect("Failed to open default device")
        }
    } else {
        println!("Using default device (pass device index as argument to change)");
        OutputStream::try_default().expect("Failed to open default device")
    };

    let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

    // Create a simple sine wave source
    println!("\nPlaying 440Hz sine wave for 3 seconds...");
    println!("(You should hear a tone)\n");

    let source = SineWave::new(440.0).take_duration(Duration::from_secs(3));

    sink.append(source);
    sink.sleep_until_end();

    println!("Done!");
}

struct SineWave {
    freq: f32,
    sample_rate: u32,
    position: u32,
}

impl SineWave {
    fn new(freq: f32) -> Self {
        Self {
            freq,
            sample_rate: 48000,
            position: 0,
        }
    }
}

impl Iterator for SineWave {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let value = (2.0 * std::f32::consts::PI * self.freq * self.position as f32 / self.sample_rate as f32).sin();
        self.position = self.position.wrapping_add(1);
        Some(value * 0.5) // 50% volume
    }
}

impl Source for SineWave {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 1 }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}
