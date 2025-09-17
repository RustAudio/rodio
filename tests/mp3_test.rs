#[cfg(any(feature = "minimp3", feature = "symphonia-mp3"))]
#[test]
fn test_silent_mp3() {
    let path = std::path::Path::new("assets/silence.mp3");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();

    // File is just silence
    assert!(decoder.all(|x| x < 0.0001));
}
