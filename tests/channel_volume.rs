use std::fs;
use std::io::BufReader;

use rodio::source::ChannelVolume;
use rodio::{queue, Decoder, Sample, Source};

fn create_6_channel_source() -> ChannelVolume<Decoder<BufReader<fs::File>>> {
    let file = fs::File::open("assets/music.mp3").unwrap();
    let decoder = Decoder::try_from(file).unwrap();
    assert_eq!(decoder.channels().get(), 2);
    let channel_volume = ChannelVolume::new(decoder, vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(channel_volume.channels().get(), 6);
    channel_volume
}

#[test]
fn channel_volume_without_queue() {
    let channel_volume = create_6_channel_source();
    assert_output_only_on_first_two_channels(channel_volume, 6);
}

#[test]
fn channel_volume_with_queue() {
    let channel_volume = create_6_channel_source();
    let (controls, queue) = queue::queue(false);
    controls.append(channel_volume);
    assert_output_only_on_first_two_channels(queue, 6);
}

fn assert_output_only_on_first_two_channels(
    mut source: impl Source<Item = Sample>,
    channels: usize,
) {
    let mut frame_number = 0;
    let mut samples_in_frame = Vec::new();

    while let Some(sample) = source.next() {
        samples_in_frame.push(sample);

        if samples_in_frame.len() == channels {
            // We have a complete frame - verify channels 2+ are zero
            for (ch, &sample) in samples_in_frame[2..].iter().enumerate() {
                assert_eq!(
                    sample,
                    0.0,
                    "frame {} channel {} had nonzero value (should be zero)",
                    frame_number,
                    ch + 2
                );
            }

            samples_in_frame.clear();
            frame_number += 1;
        }
    }

    assert_eq!(
        samples_in_frame.len(),
        0,
        "Source ended with partial frame {} (should end on frame boundary)",
        samples_in_frame.len()
    );
}
