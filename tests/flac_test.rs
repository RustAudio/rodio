use rodio;

use rodio::Source;
use std::{io::BufReader, time::Duration};

#[test]
fn test_flac_encodings() {
    // 16 bit FLAC file exported from Audacity (2 channels, compression level 5)
    let file = std::fs::File::open("tests/audacity16bit_level5.flac").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(decoder.any(|x| x != 0)); // File is not just silence
    assert_eq!(decoder.total_duration(), Some(Duration::from_secs(3))); // duration is calculated correctly

    // 24 bit FLAC file exported from Audacity (2 channels, various compression levels)
    for level in &[0, 5, 8] {
        let file = std::fs::File::open(format!("tests/audacity24bit_level{}.flac", level)).unwrap();
        let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
        assert!(decoder.any(|x| x != 0));
        assert_eq!(decoder.total_duration(), Some(Duration::from_secs(3)));
    }
}
