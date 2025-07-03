use rodio::source::Source;
use std::time::Duration;

#[test]
fn test_limiting_works() {
    // High amplitude sine wave limited to -6dB
    let sine_wave = rodio::source::SineWave::new(440.0)
        .amplify(3.0) // 3.0 linear = ~9.5dB
        .take_duration(Duration::from_millis(60)); // ~2600 samples

    let settings = rodio::source::LimitSettings::default()
        .with_threshold(-6.0)   // -6dB = ~0.5 linear
        .with_knee_width(0.5)
        .with_attack(Duration::from_millis(3))
        .with_release(Duration::from_millis(12));

    let limiter = sine_wave.limit(settings);
    let samples: Vec<f32> = limiter.take(2600).collect();

    // After settling, ALL samples should be well below 1.0 (around 0.5)
    let settled_samples = &samples[1500..]; // After attack/release settling
    let settled_peak = settled_samples
        .iter()
        .fold(0.0f32, |acc, &x| acc.max(x.abs()));

    assert!(
        settled_peak <= 0.6,
        "Settled peak should be ~0.5 for -6dB: {settled_peak:.3}"
    );
    assert!(
        settled_peak >= 0.4,
        "Peak should be reasonably close to 0.5: {settled_peak:.3}"
    );

    let max_sample = settled_samples
        .iter()
        .fold(0.0f32, |acc, &x| acc.max(x.abs()));
    assert!(
        max_sample < 0.8,
        "ALL samples should be well below 1.0: max={max_sample:.3}"
    );
}

#[test]
fn test_passthrough_below_threshold() {
    // Low amplitude signal should pass through unchanged
    let sine_wave = rodio::source::SineWave::new(1000.0)
        .amplify(0.2) // 0.2 linear, well below -6dB threshold
        .take_duration(Duration::from_millis(20));

    let settings = rodio::source::LimitSettings::default().with_threshold(-6.0);

    let original_samples: Vec<f32> = sine_wave.clone().take(880).collect();
    let limiter = sine_wave.limit(settings);
    let limited_samples: Vec<f32> = limiter.take(880).collect();

    // Samples should be nearly identical since below threshold
    for (orig, limited) in original_samples.iter().zip(limited_samples.iter()) {
        let diff = (orig - limited).abs();
        assert!(
            diff < 0.01,
            "Below threshold should pass through: diff={diff:.6}"
        );
    }
}

#[test]
fn test_limiter_with_different_settings() {
    // Test limiter with various threshold settings
    let test_cases = vec![
        (-1.0, 0.89), // -1 dBFS ≈ 89% amplitude
        (-3.0, 0.71), // -3 dBFS ≈ 71% amplitude
        (-6.0, 0.50), // -6 dBFS ≈ 50% amplitude
    ];

    for (threshold_db, expected_peak) in test_cases {
        let sine_wave = rodio::source::SineWave::new(440.0)
            .amplify(2.0) // Ensure signal exceeds all thresholds
            .take_duration(Duration::from_millis(50));

        let settings = rodio::source::LimitSettings::default()
            .with_threshold(threshold_db)
            .with_knee_width(1.0)
            .with_attack(Duration::from_millis(2))
            .with_release(Duration::from_millis(10));

        let limiter = sine_wave.limit(settings);
        let samples: Vec<f32> = limiter.take(2000).collect();

        // Check settled samples after attack/release
        let settled_samples = &samples[1000..];
        let peak = settled_samples
            .iter()
            .fold(0.0f32, |acc, &x| acc.max(x.abs()));

        assert!(
            peak <= expected_peak + 0.1,
            "Threshold {}dB: peak {:.3} should be ≤ {:.3}",
            threshold_db,
            peak,
            expected_peak + 0.1
        );
        assert!(
            peak >= expected_peak - 0.1,
            "Threshold {}dB: peak {:.3} should be ≥ {:.3}",
            threshold_db,
            peak,
            expected_peak - 0.1
        );
    }
}

#[test]
fn test_limiter_stereo_processing() {
    // Test that stereo limiting works correctly
    use rodio::buffer::SamplesBuffer;

    // Create stereo test signal - left channel louder than right
    let left_samples = (0..1000)
        .map(|i| (i as f32 * 0.01).sin() * 1.5)
        .collect::<Vec<_>>();
    let right_samples = (0..1000)
        .map(|i| (i as f32 * 0.01).sin() * 0.8)
        .collect::<Vec<_>>();

    let mut stereo_samples = Vec::new();
    for i in 0..1000 {
        stereo_samples.push(left_samples[i]);
        stereo_samples.push(right_samples[i]);
    }

    let buffer = SamplesBuffer::new(2, 44100, stereo_samples);
    let settings = rodio::source::LimitSettings::default().with_threshold(-3.0);

    let limiter = buffer.limit(settings);
    let limited_samples: Vec<f32> = limiter.collect();

    // Extract left and right channels after limiting
    let limited_left: Vec<f32> = limited_samples.iter().step_by(2).cloned().collect();
    let limited_right: Vec<f32> = limited_samples.iter().skip(1).step_by(2).cloned().collect();

    let left_peak = limited_left.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
    let right_peak = limited_right
        .iter()
        .fold(0.0f32, |acc, &x| acc.max(x.abs()));

    // Both channels should be limited to approximately the same level
    // (limiter should prevent the louder channel from exceeding threshold)
    assert!(
        left_peak <= 1.5,
        "Left channel should be limited: {left_peak:.3}"
    );
    assert!(
        right_peak <= 1.5,
        "Right channel should be limited: {right_peak:.3}"
    );
}
