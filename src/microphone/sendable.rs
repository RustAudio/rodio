//! Slightly less efficient microphone that multiple sources can draw from
//! think of it as an inverse mixer.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::Device;
use rtrb::RingBuffer;

use crate::{microphone::open_input_stream, Source};
use crate::{
    microphone::{InputConfig, OpenError},
    Sample,
};

/// Send on all platforms
pub struct Microphone {
    _stream_thread: JoinHandle<()>,
    buffer: rtrb::Consumer<Sample>,
    config: InputConfig,
    poll_interval: Duration,
    error_occurred: Arc<AtomicBool>,
    _drop_tx: mpsc::Sender<()>,
}

impl Microphone {
    pub(crate) fn open(
        device: Device,
        config: InputConfig,
        mut error_callback: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> Result<Self, OpenError> {
        let hundred_ms_of_samples =
            config.channel_count.get() as u32 * config.sample_rate.get() / 10;
        let (tx, rx) = RingBuffer::new(hundred_ms_of_samples as usize);
        let error_occurred = Arc::new(AtomicBool::new(false));
        let error_callback = {
            let error_occurred = error_occurred.clone();
            move |source| {
                error_occurred.store(true, Ordering::Relaxed);
                error_callback(source);
            }
        };

        let (res_tx, res_rx) = mpsc::channel();
        let (_drop_tx, drop_rx) = mpsc::channel::<()>();
        let _stream_thread = thread::Builder::new()
            .name("Rodio cloneable microphone".to_string())
            .spawn(move || {
                if let Err(e) = open_input_stream(device, config, tx, error_callback) {
                    let _ = res_tx.send(Err(e));
                } else {
                    let _ = res_tx.send(Ok(()));
                };

                let _should_drop = drop_rx.recv();
            })
            .expect("Should be able to spawn threads");

        res_rx
            .recv()
            .expect("input stream thread should never panic")?;

        Ok(Microphone {
            _stream_thread,
            _drop_tx,
            buffer: rx,
            config,
            poll_interval: Duration::from_millis(5),
            error_occurred,
        })
    }

    /// Get the configuration.
    ///
    /// # Example
    /// Print the sample rate and channel count.
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mic = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .open_stream()?;
    /// let config = mic.config();
    /// println!("Sample rate: {}", config.sample_rate.get());
    /// println!("Channel count: {}", config.channel_count.get());
    /// # Ok(())
    /// # }
    /// ```
    pub fn config(&self) -> &InputConfig {
        &self.config
    }
}

impl Source for Microphone {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> crate::ChannelCount {
        self.config.channel_count
    }

    fn sample_rate(&self) -> crate::SampleRate {
        self.config.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Iterator for Microphone {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Ok(sample) = self.buffer.pop() {
                return Some(sample);
            } else if self.error_occurred.load(Ordering::Relaxed) {
                return None;
            } else {
                thread::sleep(self.poll_interval)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.buffer.slots(), None)
    }
}
