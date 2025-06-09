#[cfg(any(feature = "hound", feature = "wav"))]
#[test]
fn test_wav_encodings() {
    // 16 bit wav file exported from Audacity (1 channel)
    let file = std::fs::File::open("assets/audacity16bit.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 16 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms16bit.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 24 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms24bit.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit wav file exported from Audacity (1 channel)
    let file = std::fs::File::open("assets/audacity32bit.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms32bit.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));

    // 32 bit signed integer wav file exported from Audacity (1 channel).
    let file = std::fs::File::open("assets/audacity32bit_int.wav").unwrap();
    let mut decoder = rodio::Decoder::try_from(file).unwrap();
    assert!(decoder.any(|x| x != 0.0));
}
