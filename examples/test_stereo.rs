// Test stereo audio playback (matching main app's format)
use std::time::Duration;
use rodio::{OutputStream, Sink, Source};
use rodio::cpal::traits::{HostTrait, DeviceTrait};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

fn main() {
    println!("Testing STEREO audio playback (48kHz, 2 channels)...\n");

    let host = rodio::cpal::default_host();
    println!("Available output devices:");
    let devices: Vec<_> = host.output_devices()
        .map(|d| d.collect())
        .unwrap_or_default();

    for (i, device) in devices.iter().enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("  {}: {}", i, name);
    }

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
        println!("Using default device");
        OutputStream::try_default().expect("Failed to open default device")
    };

    let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

    println!("\nPlaying 440Hz STEREO sine wave for 3 seconds...");
    println!("Format: {} Hz, {} channels (same as main app)", SAMPLE_RATE, CHANNELS);
    println!("(You should hear a tone)\n");

    let source = StereoSineWave::new(440.0).take_duration(Duration::from_secs(3));

    sink.append(source);

    // Check sink status
    println!("Sink empty: {}", sink.empty());
    println!("Sink paused: {}", sink.is_paused());

    sink.sleep_until_end();

    println!("Done!");
}

struct StereoSineWave {
    freq: f32,
    sample_rate: u32,
    position: u32,
    channel: u16,  // Alternates between 0 and 1 for stereo
}

impl StereoSineWave {
    fn new(freq: f32) -> Self {
        Self {
            freq,
            sample_rate: SAMPLE_RATE,
            position: 0,
            channel: 0,
        }
    }
}

impl Iterator for StereoSineWave {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let value = (2.0 * std::f32::consts::PI * self.freq * self.position as f32 / self.sample_rate as f32).sin();

        // Advance position only after both channels
        self.channel += 1;
        if self.channel >= CHANNELS {
            self.channel = 0;
            self.position = self.position.wrapping_add(1);
        }

        Some(value * 0.5) // 50% volume
    }
}

impl Source for StereoSineWave {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { CHANNELS }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}
