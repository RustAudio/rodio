//! Slightly less efficient microphone that multiple sources can draw from
//! think of it as an inverse mixer.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
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
    error_occurred: Arc<AtomicBool>,
    data_signal: Arc<(Mutex<()>, Condvar)>,
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
        // Using rtrb (real-time ring buffer) instead of std::sync::mpsc or the ringbuf crate for
        // audio performance. While ringbuf has Send variants that could eliminate the need for
        // separate sendable/non-sendable microphone implementations, rtrb has been benchmarked to
        // be significantly faster in throughput and provides lower latency operations.
        let (tx, rx) = RingBuffer::new(hundred_ms_of_samples as usize);
        let error_occurred = Arc::new(AtomicBool::new(false));
        let data_signal = Arc::new((Mutex::new(()), Condvar::new()));
        let error_callback = {
            let error_occurred = error_occurred.clone();
            let data_signal = data_signal.clone();
            move |source| {
                error_occurred.store(true, Ordering::Relaxed);
                let (_lock, cvar) = &*data_signal;
                cvar.notify_one();
                error_callback(source);
            }
        };

        let (res_tx, res_rx) = mpsc::channel();
        let (_drop_tx, drop_rx) = mpsc::channel::<()>();
        let data_signal_clone = data_signal.clone();
        let _stream_thread = thread::Builder::new()
            .name("Rodio cloneable microphone".to_string())
            .spawn(move || {
                match open_input_stream(device, config, tx, error_callback, data_signal_clone) {
                    Err(e) => {
                        let _ = res_tx.send(Err(e));
                    }
                    Ok(_) => {
                        let _ = res_tx.send(Ok(()));
                        // Keep the stream alive until we're told to drop
                        let _should_drop = drop_rx.recv();
                    }
                }
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
            error_occurred,
            data_signal,
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
                // Block until notified instead of sleeping. This eliminates polling overhead and
                // reduces jitter by avoiding unnecessary  wakeups when no audio data is available.
                let (lock, cvar) = &*self.data_signal;
                let guard = lock.lock().unwrap();
                let _guard = cvar.wait(guard).unwrap();
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.buffer.slots(), None)
    }
}
