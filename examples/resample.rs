//! Example demonstrating audio resampling with different quality presets.

use rodio::source::{resample::Poly, ResampleConfig, Source};
use rodio::{Decoder, Player};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(debug_assertions)]
    {
        eprintln!("WARNING: Running in debug mode. Audio may be choppy, especially with");
        eprintln!("         sinc resampling of non-integer ratios (async resampling).");
        eprintln!("         For best results, compile with --release");
        eprintln!();
    }

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <target_rate> [audio_file] [method]", args[0]);
        eprintln!("\nTarget rate: Sample rate in Hz (e.g., 48000, 96000)");
        eprintln!("\nAudio file (optional): Path to audio file (default: assets/music.ogg)");
        eprintln!("\nMethod (optional): nearest, linear, fast, balanced, accurate");
        eprintln!("\nMethod details:");
        eprintln!("  nearest   - Zero-order hold (non-oversampling), fastest");
        eprintln!("  linear    - Linear polynomial interpolation, fast");
        eprintln!("  very_fast - 64-tap sinc, linear interpolation, Hann2 window");
        eprintln!("  fast      - 128-tap sinc, linear interpolation, Hann2 window");
        eprintln!("  balanced  - 192-tap sinc, linear interpolation, Blackman2 window (default)");
        eprintln!("  accurate  - 256-tap sinc, cubic interpolation, BlackmanHarris2 window");
        eprintln!("\nExamples:");
        eprintln!("  {} 48000", args[0]);
        eprintln!("  {} 96000 assets/music.ogg accurate", args[0]);
        std::process::exit(1);
    }

    let target_rate: u32 = args[1].parse().map_err(|_| {
        format!(
            "Invalid target rate '{}'. Must be a positive integer (e.g., 48000)",
            args[1]
        )
    })?;

    if target_rate == 0 {
        return Err("Target rate must be greater than 0".into());
    }

    let audio_path = if args.len() > 2 {
        args[2].clone()
    } else {
        "assets/music.ogg".to_string()
    };

    let config = if args.len() > 3 {
        parse_quality(&args[3])?
    } else {
        ResampleConfig::default()
    };

    println!("Audio file: {audio_path}");

    let file = File::open(&audio_path)
        .map_err(|e| format!("Failed to open audio file '{audio_path}': {e}"))?;
    let source = Decoder::try_from(BufReader::new(file))?;

    let source_rate = source.sample_rate().get();
    let channels = source.channels().get();
    let duration = source.total_duration();

    if let Some(dur) = duration {
        println!("Duration: {:.2}s", dur.as_secs_f32());
    }

    println!("\nResampling {channels} channels from {source_rate} Hz to {target_rate} Hz...");
    println!("Configuration: {config:#?}");
    let resampled = source.resample(rodio::SampleRate::new(target_rate).unwrap(), config);

    println!("\nConfiguring output device to {target_rate} Hz...");
    let stream_handle = rodio::DeviceSinkBuilder::from_default_device()?
        .with_sample_rate(rodio::SampleRate::new(target_rate).unwrap())
        .open_stream()?;
    let player = Player::connect_new(stream_handle.mixer());

    println!("Playing resampled audio...");
    println!("Press Ctrl+C to stop");

    let playback_start = Instant::now();
    player.append(resampled);
    player.sleep_until_end();

    let playback_time = playback_start.elapsed();
    println!("\nPlayback finished in {:.2}s", playback_time.as_secs_f32());

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
        "very_fast" => ResampleConfig::very_fast(),
        "fast" => ResampleConfig::fast(),
        "balanced" => ResampleConfig::balanced(),
        "accurate" => ResampleConfig::accurate(),
        _ => return Err(format!(
            "Unknown resampling method '{}'. Valid options: nearest, linear, cubic, quintic, septic, very_fast, fast, balanced, accurate",
            method
        )
        .into()),
    };
    Ok(config)
}
