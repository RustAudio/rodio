#[cfg(any(feature = "hound", feature = "wav"))]
#[test]
fn test_wav_encodings() {
    // 16 bit wav file exported from Audacity (1 channel)
    let path = std::path::Path::new("assets/audacity16bit.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 16 bit wav file exported from LMMS (2 channels)
    let path = std::path::Path::new("assets/lmms16bit.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 24 bit wav file exported from LMMS (2 channels)
    let path = std::path::Path::new("assets/lmms24bit.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit wav file exported from Audacity (1 channel)
    let path = std::path::Path::new("assets/audacity32bit.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit wav file exported from LMMS (2 channels)
    let path = std::path::Path::new("assets/lmms32bit.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit signed integer wav file exported from Audacity (1 channel).
    let path = std::path::Path::new("assets/audacity32bit_int.wav");
    let mut decoder = rodio::Decoder::try_from(path).unwrap();
    assert!(decoder.any(|x| x != 0.0));
}
