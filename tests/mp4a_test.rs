#![cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]

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
