#![cfg(feature = "loudness")]

use std::num::NonZero;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::source::Source;
use rodio::Sample;

/// One second is comfortably longer than the 400 ms momentary window, so the
/// reading has settled.
fn one_second(freq: f32) -> impl Source {
    rodio::source::SineWave::new(freq).take_duration(Duration::from_secs(1))
}

#[test]
fn loud_sine_reads_plausible_lufs() {
    let mut metered = one_second(1000.0).loudness();
    // Drive the whole second through the meter.
    let _: Vec<Sample> = metered.by_ref().collect();

    let momentary = metered.momentary_lufs();
    let integrated = metered.integrated_lufs();

    // A full-scale sine sits a few LUFS below 0; well within a sane band.
    assert!(
        momentary.is_finite() && (-30.0..0.0).contains(&momentary),
        "momentary out of range: {momentary}"
    );
    assert!(
        integrated.is_finite() && (-30.0..0.0).contains(&integrated),
        "integrated out of range: {integrated}"
    );
}

#[test]
fn silence_is_negative_infinity() {
    let channels = NonZero::new(1).unwrap();
    let rate = NonZero::new(48_000).unwrap();
    let silence: Vec<Sample> = vec![0.0; 48_000];
    let silence = SamplesBuffer::new(channels, rate, silence);

    let mut metered = silence.loudness();
    let _: Vec<Sample> = metered.by_ref().collect();

    assert_eq!(metered.integrated_lufs(), f64::NEG_INFINITY);
}

#[test]
fn louder_signal_reads_higher() {
    let quiet = {
        let mut m = one_second(1000.0).amplify(0.1).loudness();
        let _: Vec<Sample> = m.by_ref().collect();
        m.integrated_lufs()
    };
    let loud = {
        let mut m = one_second(1000.0).amplify(0.5).loudness();
        let _: Vec<Sample> = m.by_ref().collect();
        m.integrated_lufs()
    };

    assert!(
        loud > quiet,
        "louder should read higher: loud={loud}, quiet={quiet}"
    );
}

#[test]
fn stereo_source_is_measured_and_passed_through() {
    // Interleaved stereo: identical tone in both channels.
    let frames = 48_000;
    let mut samples: Vec<Sample> = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let s = (i as Sample * 0.05).sin() * 0.5;
        samples.push(s); // left
        samples.push(s); // right
    }

    let channels = NonZero::new(2).unwrap();
    let rate = NonZero::new(48_000).unwrap();
    let buffer = SamplesBuffer::new(channels, rate, samples.clone());

    let mut metered = buffer.loudness();
    let passed: Vec<Sample> = metered.by_ref().collect();

    assert_eq!(passed, samples, "stereo audio must pass through unchanged");
    assert!(
        metered.integrated_lufs().is_finite(),
        "stereo loudness should be finite"
    );
}
