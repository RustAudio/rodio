#[cfg(any(feature = "claxon", feature = "symphonia-flac"))]
use rodio::Source;
#[cfg(any(feature = "claxon", feature = "symphonia-flac"))]
use std::time::Duration;

#[cfg(any(feature = "claxon", feature = "symphonia-flac"))]
#[test]
fn test_flac_encodings() {
    // 16 bit FLAC file exported from Audacity (2 channels, compression level 5)
    let file = std::fs::File::open("assets/audacity16bit_level5.flac").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();

    // File is not just silence
    assert!(decoder.any(|x| x != 0.0));
    assert_eq!(decoder.total_duration(), Some(Duration::from_secs(3))); // duration is calculated correctly

    // 24 bit FLAC file exported from Audacity (2 channels, various compression levels)
    for level in &[0, 5, 8] {
        let file = std::fs::File::open(format!("assets/audacity24bit_level{level}.flac")).unwrap();
        let mut decoder = rodio::Decoder::try_from(file).unwrap();
        assert!(!decoder.all(|x| x != 0.0));
        assert_eq!(decoder.total_duration(), Some(Duration::from_secs(3)));
    }
}
