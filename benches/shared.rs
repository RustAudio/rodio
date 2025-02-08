use std::io::Cursor;
use std::time::Duration;
use std::vec;

use rodio::{decoder::DecoderSample, ChannelCount, Sample, SampleRate, Source};

pub struct TestSource {
    samples: vec::IntoIter<Sample>,
    channels: u16,
    sample_rate: u32,
    total_duration: Duration,
}

impl Iterator for TestSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.samples.next()
    }
}

impl ExactSizeIterator for TestSource {
    #[inline]
    fn len(&self) -> usize {
        self.samples.len()
    }
}

impl Source for TestSource {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None // forever
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }
}

// impl TestSource<i16> {
//     #[allow(unused, reason = "not everything from shared is used in all libs")]
//     pub fn to_f32s(self) -> TestSource<f32> {
//         let TestSource {
//             samples,
//             channels,
//             sample_rate,
//             total_duration,
//         } = self;
//         let samples = samples.collect::<Vec<_>>().into_iter();
//         TestSource {
//             samples,
//             channels,
//             sample_rate,
//             total_duration,
//         }
//     }
// }
//
// impl TestSource<f32> {
//     #[allow(unused, reason = "not everything from shared is used in all libs")]
//     pub fn to_f32s(self) -> TestSource<f32> {
//         self
//     }
// }

pub fn music_wav() -> TestSource {
    let data = include_bytes!("../assets/music.wav");
    let cursor = Cursor::new(data);

    let duration = Duration::from_secs(10);
    let sound = rodio::Decoder::new(cursor)
        .expect("music.wav is correctly encoded & wav is supported")
        .take_duration(duration);

    TestSource {
        channels: sound.channels(),
        sample_rate: sound.sample_rate(),
        total_duration: duration,
        samples: sound.into_iter().collect::<Vec<_>>().into_iter(),
    }
}
