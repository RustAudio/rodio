use std::io::Cursor;
use std::time::Duration;
use std::vec;

use rodio::Source;

pub struct TestSource<T> {
    samples: vec::IntoIter<T>,
    channels: u16,
    sample_rate: u32,
    total_duration: Duration,
}

impl<T> Iterator for TestSource<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.samples.next()
    }
}

impl<T> ExactSizeIterator for TestSource<T> {
    fn len(&self) -> usize {
        self.samples.len()
    }
}

impl<T: rodio::Sample> Source for TestSource<T> {
    fn current_frame_len(&self) -> Option<usize> {
        None // forever
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }
}

impl TestSource<i16> {
    pub fn music_wav() -> Self {
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

    #[allow(unused, reason = "not everything from shared is used in all libs")]
    pub fn to_f32s(self) -> TestSource<f32> {
        let TestSource {
            samples,
            channels,
            sample_rate,
            total_duration,
        } = self;
        let samples = samples
            .map(|s| cpal::Sample::from_sample(s))
            .collect::<Vec<_>>()
            .into_iter();
        TestSource {
            samples,
            channels,
            sample_rate,
            total_duration,
        }
    }
}
