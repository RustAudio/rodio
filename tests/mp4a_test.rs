#![cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]
use std::io::BufReader;

#[test]
fn test_mp4a_encodings() {
    // mp4a codec downloaded from YouTube
    // "Monkeys Spinning Monkeys"
    // Kevin MacLeod (incompetech.com)
    // Licensed under Creative Commons: By Attribution 3.0
    // http://creativecommons.org/licenses/by/3.0/
    let file = std::fs::File::open("assets/monkeys.mp4a").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(decoder.any(|x| x != 0)); // Assert not all zeros
}
