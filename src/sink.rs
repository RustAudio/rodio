use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::Mutex;

use play_raw;
use queue;
use source;
use Endpoint;
use Source;
use Sample;

/// Handle to an endpoint that outputs sounds.
///
/// Dropping the `Sink` stops all sounds. You can use `detach` if you want the sounds to continue
/// playing.
pub struct Sink {
    queue_tx: Arc<queue::SourcesQueueInput<f32>>,
    sleep_until_end: Mutex<Option<Receiver<()>>>,

    pause: Arc<AtomicBool>,
    volume: Arc<Mutex<f32>>,
    stopped: Arc<AtomicBool>,

    detached: bool,
}

impl Sink {
    /// Builds a new `Sink`.
    #[inline]
    pub fn new(endpoint: &Endpoint) -> Sink {
        let (queue_tx, queue_rx) = queue::queue(true);
        play_raw(endpoint, queue_rx);

        Sink {
            queue_tx: queue_tx,
            sleep_until_end: Mutex::new(None),
            pause: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(Mutex::new(1.0)),
            stopped: Arc::new(AtomicBool::new(false)),
            detached: false,
        }
    }

    /// Appends a sound to the queue of sounds to play.
    #[inline]
    pub fn append<S>(&self, source: S)
        where S: Source + Send + 'static,
              S::Item: Sample,
              S::Item: Send
    {
        let source = source::Pauseable::new(source, self.pause.clone(), 5);
        let source = source::Stoppable::new(source, self.stopped.clone(), 5);
        let source = source::VolumeFilter::new(source, self.volume.clone(), 5);
        let source = source::SamplesConverter::new(source);
        *self.sleep_until_end.lock().unwrap() = Some(self.queue_tx.append_with_signal(source));
    }

    // Gets the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }

    /// Changes the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn set_volume(&mut self, value: f32) {
        *self.volume.lock().unwrap() = value;
    }

    /// Resumes playback of a paused sound.
    ///
    /// No effect if not paused.
    #[inline]
    pub fn play(&self) {
        self.pause.store(false, Ordering::SeqCst);
    }

    /// Pauses playback of this sink.
    ///
    /// No effect if already paused.
    ///
    /// A paused sound can be resumed with `play()`.
    pub fn pause(&self) {
        self.pause.store(true, Ordering::SeqCst);
    }

    /// Gets if a sound is paused
    ///
    /// Sounds can be paused and resumed using pause() and play(). This gets if a sound is paused.
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::SeqCst)
    }

    /// Destroys the sink without stopping the sounds that are still playing.
    #[inline]
    pub fn detach(mut self) {
        self.detached = true;
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        if let Some(sleep_until_end) = self.sleep_until_end.lock().unwrap().take() {
            let _ = sleep_until_end.recv();
        }
    }
}

impl Drop for Sink {
    #[inline]
    fn drop(&mut self) {
        self.queue_tx.set_keep_alive_if_empty(false);

        if !self.detached {
            self.stopped.store(true, Ordering::Relaxed);
        }
    }
}
