#[cfg(feature = "wav")]
use rodio::Sample;

#[cfg(feature = "wav")]
#[test]
fn test_wav_encodings() {
    use std::io::BufReader;

    // 16 bit wav file exported from Audacity (1 channel)
    let file = std::fs::File::open("assets/audacity16bit.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero())); // Assert not all zeros

    // 16 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms16bit.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero()));

    // 24 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms24bit.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero()));

    // 32 bit wav file exported from Audacity (1 channel)
    let file = std::fs::File::open("assets/audacity32bit.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero()));

    // 32 bit wav file exported from LMMS (2 channels)
    let file = std::fs::File::open("assets/lmms32bit.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero()));

    // 32 bit signed integer wav file exported from Audacity (1 channel).
    let file = std::fs::File::open("assets/audacity32bit_int.wav").unwrap();
    let mut decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert!(!decoder.all(|x| x.is_zero()));
}
