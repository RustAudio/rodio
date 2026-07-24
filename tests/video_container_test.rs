//! Regression tests for decoding the audio track of a container whose
//! *default* track is not the audio track — i.e. a video file.
//!
//! `assets/video_with_audio.mp4` holds a 2 s H.264 video track (the default,
//! 60 frames) plus a 2 s mono AAC audio track.

#![cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]

use rodio::{Decoder, Source};

fn decode_video_asset() -> Decoder<std::fs::File> {
    let file = std::fs::File::open("assets/video_with_audio.mp4").unwrap();
    let len = file.metadata().unwrap().len();
    Decoder::builder()
        .with_data(file)
        .with_byte_len(len)
        .with_seekable(true)
        .build()
        .unwrap()
}

/// `total_duration` must come from the audio track (~2.02 s), not the video
/// track. The buggy pairing reported ~0.7 s, so a generous lower bound alone
/// distinguishes the two, and the tight check pins the correct value.
#[test]
fn reports_audio_track_duration_not_default_track() {
    let duration = decode_video_asset()
        .total_duration()
        .expect("video container should report a total duration")
        .as_secs_f64();
    assert!(
        duration > 1.5,
        "duration {duration}s looks like the video track's frame count, not the audio track"
    );
    let expected = 2.023_219_954;
    assert!(
        (duration - expected).abs() < 0.01,
        "got {duration}s, expected about {expected}s"
    );
}
