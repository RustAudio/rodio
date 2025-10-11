#![cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]

use rodio::{Decoder, Source};
use std::path::Path;
use std::time::Duration;

#[test]
fn test_mp4a_encodings() {
    // mp4a codec downloaded from YouTube
    // "Monkeys Spinning Monkeys"
    // Kevin MacLeod (incompetech.com)
    // Licensed under Creative Commons: By Attribution 3.0
    // http://creativecommons.org/licenses/by/3.0/
    let file = std::fs::File::open("assets/monkeys.mp4a").unwrap();
    // Open with `new` instead of `try_from` to ensure it works even without is_seekable
    let mut decoder = rodio::Decoder::new(file).unwrap();
    assert!(decoder.any(|x| x != 0.0)); // Assert not all zeros
}

#[test]
fn test_m4a_zero_frames_handling() {
    // Test for AAC M4A files that report n_frames=0 but contain valid audio data.
    // Some AAC M4A files (like ISO Media MP4 Base Media v5) incorrectly report
    // n_frames=0 in metadata, which should be handled by setting duration to None
    // instead of Some(0ns) to allow proper seeking functionality.

    let path = Path::new("assets/iso5.m4a");
    let mut decoder = Decoder::try_from(path).unwrap();

    // Files with n_frames=0 should report None duration, not Some(0ns)
    assert_ne!(
        decoder.total_duration(),
        Some(Duration::ZERO),
        "Files with n_frames=0 should not report Some(0ns) duration"
    );

    // Should be able to decode and get valid audio samples
    let initial_samples: Vec<f32> = decoder.by_ref().take(100).collect();
    assert!(!initial_samples.is_empty(), "Should get audio samples");

    // Test forward and backward seeking
    let seek_positions = [
        Duration::from_secs(2), // Forward seek
        Duration::from_secs(5), // Further forward
        Duration::from_secs(1), // Backward seek
        Duration::from_secs(8), // Forward again
        Duration::from_secs(3), // Backward again
    ];

    let mut previous_samples: Option<Vec<f32>> = None;
    for &pos in &seek_positions {
        decoder.try_seek(pos).expect("Seeking should work");

        let seek_samples: Vec<f32> = decoder.by_ref().take(50).collect();
        assert!(
            !seek_samples.is_empty(),
            "Should get samples after seek to {:?}",
            pos
        );

        // Ensure samples are not all zeros
        let non_zero_count = seek_samples.iter().filter(|&&s| s.abs() > 0.001).count();
        assert!(
            non_zero_count > 0,
            "Should have non-zero audio samples after seeking to {:?}, got all zeros",
            pos
        );

        // Ensure samples are different from previous position
        if let Some(ref prev) = previous_samples {
            let samples_different = prev
                .iter()
                .zip(seek_samples.iter())
                .any(|(a, b)| (a - b).abs() > 0.001);

            assert!(
                samples_different,
                "Samples at {:?} should be different from previous seek position",
                pos
            );
        }

        previous_samples = Some(seek_samples);
    }
}
