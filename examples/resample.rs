//! Example demonstrating audio resampling with different quality presets.
//!
//! Usage:
//!   cargo run --release --example resample <target_rate> [audio_file] [method]
//!
//! Arguments:
//!   target_rate  - Target sample rate in Hz (required, e.g., 96000, 192000)
//!   audio_file   - Optional path to audio file (default: assets/music.ogg)
//!   method       - Optional resampling method (default: balanced)
//!                  Polynomial: nearest, linear, cubic, quintic, septic
//!                  Sinc: fast, balanced, accurate
//!
//! Examples:
//!   cargo run --release --example resample 96000
//!   cargo run --release --example resample 96000 assets/music.ogg accurate
//!   cargo run --release --example resample 192000 assets/music.ogg septic

use rodio::source::{resample::Poly, Resample, ResampleConfig, Source};
use rodio::{Decoder, Player};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <target_rate> [audio_file] [method]", args[0]);
        eprintln!("\nTarget rate: Sample rate in Hz (e.g., 48000, 96000)");
        eprintln!("\nAudio file (optional): Path to audio file (default: assets/music.ogg)");
        eprintln!("\nMethod (optional): nearest, linear, fast, balanced, accurate");
        eprintln!("\nMethod details:");
        eprintln!("  nearest   - Zero-order hold (NOS DAC simulation), fastest");
        eprintln!("  linear    - Linear polynomial interpolation, fast");
        eprintln!("  fast      - 64-tap sinc, linear interpolation, Hann2 window");
        eprintln!("  balanced  - 128-tap sinc, linear interpolation, Blackman2 window (default)");
        eprintln!("  accurate  - 256-tap sinc, cubic interpolation, BlackmanHarris2 window");
        eprintln!("\nNote: Higher quality sinc = better preservation of high frequencies near Nyquist limit.");
        eprintln!("      Quality levels control filter sharpness, trading CPU/latency for frequency response.");
        eprintln!("\nExamples:");
        eprintln!("  {} 48000", args[0]);
        eprintln!("  {} 96000 assets/music.ogg accurate", args[0]);
        std::process::exit(1);
    }

    // Parse target sample rate
    let target_rate: u32 = args[1].parse().map_err(|_| {
        format!(
            "Invalid target rate '{}'. Must be a positive integer (e.g., 48000)",
            args[1]
        )
    })?;

    if target_rate == 0 {
        return Err("Target rate must be greater than 0".into());
    }

    // Get audio file path (default to music.ogg)
    let audio_path = if args.len() > 2 {
        args[2].clone()
    } else {
        "assets/music.ogg".to_string()
    };

    // Parse resampling method
    let config = if args.len() > 3 {
        parse_quality(&args[3])?
    } else {
        ResampleConfig::balanced() // Default
    };

    println!("=== Rodio Resampling Example ===");
    println!("Audio file: {}", audio_path);
    println!("Target rate: {} Hz", target_rate);

    // Open the audio file
    let file = File::open(&audio_path)
        .map_err(|e| format!("Failed to open audio file '{}': {}", audio_path, e))?;
    let source = Decoder::try_from(BufReader::new(file))?;

    // Get source information
    let source_rate = source.sample_rate().get();
    let channels = source.channels().get();
    let duration = source.total_duration();

    println!("Source sample rate: {} Hz", source_rate);
    println!("Channels: {}", channels);
    if let Some(dur) = duration {
        println!("Duration: {:.2}s", dur.as_secs_f64());
    }
    println!();

    // Apply resampling
    println!(
        "Resampling from {} Hz to {} Hz...",
        source_rate, target_rate
    );
    let start = Instant::now();
    println!("Method: {}", format_config(&config));
    println!();
    let resampled = Resample::new(source, rodio::SampleRate::new(target_rate).unwrap(), config);
    let setup_time = start.elapsed();

    println!(
        "Setup time: {:.2}ms (filter initialization)",
        setup_time.as_secs_f64() * 1000.0
    );
    println!();

    // Verify resampled source properties
    println!(
        "Resampled source rate: {} Hz",
        resampled.sample_rate().get()
    );
    println!("Resampled source channels: {}", resampled.channels().get());
    println!();

    // Open audio output configured to match the target sample rate
    println!("Configuring output device to {} Hz...", target_rate);
    let stream_handle = rodio::DeviceSinkBuilder::from_default_device()?
        .with_sample_rate(rodio::SampleRate::new(target_rate).unwrap())
        .open_stream()?;
    let player = Player::connect_new(stream_handle.mixer());

    // Play the resampled audio
    println!("Playing resampled audio...");
    println!("Press Ctrl+C to stop");
    println!();

    let playback_start = Instant::now();
    player.append(resampled);
    player.sleep_until_end();

    let playback_time = playback_start.elapsed();
    println!();
    println!("Playback finished in {:.2}s", playback_time.as_secs_f64());

    Ok(())
}

/// Parse the resampling quality from a string argument
fn parse_quality(method: &str) -> Result<ResampleConfig, Box<dyn Error>> {
    let config = match method.to_lowercase().as_str() {
        "nearest" => ResampleConfig::poly().degree(Poly::Nearest).build(),
        "linear" => ResampleConfig::poly().degree(Poly::Linear).build(),
        "cubic" => ResampleConfig::poly().degree(Poly::Cubic).build(),
        "quintic" => ResampleConfig::poly().degree(Poly::Quintic).build(),
        "septic" => ResampleConfig::poly().degree(Poly::Septic).build(),
        "fast" => ResampleConfig::fast(),
        "balanced" => ResampleConfig::balanced(),
        "accurate" => ResampleConfig::accurate(),
        _ => return Err(format!(
            "Unknown resampling method '{}'. Valid options: nearest, linear, cubic, quintic, septic, fast, balanced, accurate",
            method
        )
        .into()),
    };
    Ok(config)
}

/// Format the config for display
fn format_config(config: &ResampleConfig) -> &'static str {
    match config {
        ResampleConfig::Poly { degree, .. } => match degree {
            Poly::Nearest => "Poly::Nearest (Zero-order hold, NOS DAC)",
            Poly::Linear => "Poly::Linear (1st degree, fast)",
            Poly::Cubic => "Poly::Cubic (3rd degree)",
            Poly::Quintic => "Poly::Quintic (5th degree)",
            Poly::Septic => "Poly::Septic (7th degree, best polynomial)",
        },
        ResampleConfig::Sinc { sinc_len, .. } => {
            // Identify by filter length
            match sinc_len {
                64 => "SincVeryFast (64-tap, anti-aliasing)",
                128 => "SincFast (128-tap, anti-aliasing)",
                192 => "SincBalanced (192-tap, anti-aliasing)",
                256 => "SincAccurate (256-tap, anti-aliasing)",
                _ => "Custom sinc configuration",
            }
        }
    }
}
