use std::fs;
use std::io::BufReader;

use itertools::Itertools;

use rodio::source::ChannelVolume;
use rodio::{Decoder, Source};

#[test]
fn tomato() {
    let file = fs::File::open("assets/music.mp3").unwrap();
    let decoder = Decoder::new(BufReader::new(file)).unwrap();
    assert_eq!(decoder.channels(), 2);
    let channel_volume = ChannelVolume::new(decoder, vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(channel_volume.channels(), 6);

    assert_output_only_on_channel_1_and_2(channel_volume);
}

fn assert_output_only_on_channel_1_and_2(source: impl Source<Item = i16>) {
    for (frame_number, mut frame) in source.chunks(6).into_iter().enumerate() {
        let frame: [_; 6] = frame.next_array().expect(&format!(
            "Source should contain whole frames, frame {frame_number} was partial"
        ));
        assert_eq!(
            &frame[2..],
            &[0, 0, 0, 0],
            "frame number {frame_number} had nonzero volume on channels 3,4,5 & 6"
        )
    }
}
