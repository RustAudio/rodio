#[cfg(any(feature = "minimp3", feature = "symphonia-mp3"))]
#[test]
fn test_silent_mp3() {
    let file = std::fs::File::open("assets/silence.mp3").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();

    // File is just silence
    assert!(decoder.all(|x| x < 0.0001));
}
