use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "crossbeam-channel")]
use crossbeam_channel::{Receiver, Sender};
use dasp_sample::FromSample;
#[cfg(not(feature = "crossbeam-channel"))]
use std::sync::mpsc::{Receiver, Sender};

use crate::mixer::Mixer;
use crate::source::SeekError;
use crate::Float;
use crate::{queue, source::Done, Source};

/// Handle to a device that outputs sounds.
///
/// Dropping the `Player` stops all its sounds. You can use `detach` if you want the sounds to continue
/// playing.
pub struct Player {
    queue_tx: Arc<queue::SourcesQueueInput>,
    sleep_until_end: Mutex<Option<Receiver<()>>>,

    controls: Arc<Controls>,
    sound_count: Arc<AtomicUsize>,

    detached: bool,
}

struct SeekOrder {
    pos: Duration,
    feedback: Sender<Result<(), SeekError>>,
}

impl SeekOrder {
    fn new(pos: Duration) -> (Self, Receiver<Result<(), SeekError>>) {
        #[cfg(not(feature = "crossbeam-channel"))]
        let (tx, rx) = {
            use std::sync::mpsc;
            mpsc::channel()
        };

        #[cfg(feature = "crossbeam-channel")]
        let (tx, rx) = {
            use crossbeam_channel::bounded;
            bounded(1)
        };
        (Self { pos, feedback: tx }, rx)
    }

    /// Performs the seek. `speed` is the current playback speed.
    ///
    /// [`Speed`](crate::source::Speed) multiplies the target position of its
    /// [`try_seek`](Source::try_seek) by the speed factor, so the target is divided by
    /// the speed first. That makes `self.pos` a position in the source's own timeline,
    /// independent of the playback speed.
    fn attempt<S>(self, maybe_seekable: &mut S, speed: f32)
    where
        S: Source,
    {
        let pos = if speed.is_finite() && speed > 0.0 {
            // f64 keeps full precision for long sources; an out of range result
            // falls back to the unscaled position instead of panicking.
            Duration::try_from_secs_f64(self.pos.as_secs_f64() / f64::from(speed))
                .unwrap_or(self.pos)
        } else {
            self.pos
        };
        let res = maybe_seekable.try_seek(pos);
        let _ignore_receiver_dropped = self.feedback.send(res);
    }
}

struct Controls {
    pause: AtomicBool,
    volume: Mutex<Float>,
    stopped: AtomicBool,
    speed: Mutex<f32>,
    to_clear: Mutex<u32>,
    seek: Mutex<Option<SeekOrder>>,
    position: Mutex<Duration>,
}

impl Player {
    /// Builds a new `Player`, beginning playback on a stream.
    #[inline]
    pub fn connect_new(mixer: &Mixer) -> Player {
        let (sink, source) = Player::new();
        mixer.add(source);
        sink
    }

    /// Builds a new `Player`.
    #[inline]
    pub fn new() -> (Player, queue::SourcesQueueOutput) {
        let (queue_tx, queue_rx) = queue::queue(true);

        let sink = Player {
            queue_tx,
            sleep_until_end: Mutex::new(None),
            controls: Arc::new(Controls {
                pause: AtomicBool::new(false),
                volume: Mutex::new(1.0),
                stopped: AtomicBool::new(false),
                speed: Mutex::new(1.0),
                to_clear: Mutex::new(0),
                seek: Mutex::new(None),
                position: Mutex::new(Duration::ZERO),
            }),
            sound_count: Arc::new(AtomicUsize::new(0)),
            detached: false,
        };
        (sink, queue_rx)
    }

    /// Appends a sound to the queue of sounds to play.
    #[inline]
    pub fn append<S>(&self, source: S)
    where
        S: Source + Send + 'static,
        f32: FromSample<S::Item>,
    {
        // Wait for the queue to flush then resume stopped playback
        if self.controls.stopped.load(Ordering::SeqCst) {
            if self.sound_count.load(Ordering::SeqCst) > 0 {
                self.sleep_until_end();
            }
            self.controls.stopped.store(false, Ordering::SeqCst);
        }

        let controls = self.controls.clone();

        let start_played = AtomicBool::new(false);
        let sound_count_clone = self.sound_count.clone();

        // Adapter order, outermost first (`periodic_access` wraps all of it):
        //   Done -> Stoppable -> Skippable -> Amplify -> Pausable -> Speed -> TrackPosition -> source
        //
        // `track_position()` must come before `speed()`. `Speed` does not change the samples,
        // it only reports a scaled sample rate, and `TrackPosition` divides its sample count by
        // the rate it sees. After `Speed` it would measure elapsed playback time instead of the
        // position in the media. It stays inside `pausable()` so that the silence played while
        // paused is not counted.
        //
        // The `.inner()` / `.inner_mut()` chains in the closure below follow this order.
        let source = Done::new(
            source
                .track_position()
                .speed(1.0)
                .pausable(false)
                .amplify(1.0)
                .skippable()
                .stoppable(),
            move |src| {
                if !src.inner().skipped() {
                    sound_count_clone.fetch_sub(1, Ordering::Relaxed);
                }
            },
        )
        // If you change the duration update the docs for try_seek!
        .periodic_access(Duration::from_millis(5), move |src| {
            if controls.stopped.load(Ordering::SeqCst) {
                src.inner_mut().stop();
                *controls.position.lock().unwrap() = Duration::ZERO;
            }
            {
                let mut to_clear = controls.to_clear.lock().unwrap();
                if *to_clear > 0 {
                    src.inner_mut().inner_mut().skip();
                    *to_clear -= 1;
                    *controls.position.lock().unwrap() = Duration::ZERO;
                } else {
                    // The position comes from `TrackPosition`, the innermost adapter.
                    *controls.position.lock().unwrap() = src
                        .inner() // Stoppable
                        .inner() // Skippable
                        .inner() // Amplify
                        .inner() // Pausable
                        .inner() // Speed
                        .inner() // TrackPosition
                        .get_pos();
                }
            }
            let amp = src
                .inner_mut() // Stoppable
                .inner_mut() // Skippable
                .inner_mut(); // Amplify
            amp.set_factor(*controls.volume.lock().unwrap());
            amp.inner_mut() // Pausable
                .set_paused(controls.pause.load(Ordering::SeqCst));

            let speed = *controls.speed.lock().unwrap();
            amp.inner_mut() // Pausable
                .inner_mut() // Speed
                .set_factor(speed);

            if let Some(seek) = controls.seek.lock().unwrap().take() {
                // The seek needs the current speed to undo the scaling in `Speed::try_seek`.
                seek.attempt(amp, speed)
            }
            start_played.store(true, Ordering::SeqCst);
        });

        self.sound_count.fetch_add(1, Ordering::Relaxed);
        *self.sleep_until_end.lock().unwrap() = Some(self.queue_tx.append_with_signal(source));
    }

    /// Gets the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn volume(&self) -> Float {
        *self.controls.volume.lock().unwrap()
    }

    /// Changes the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than `1.0` will
    /// multiply each sample by this value.
    #[inline]
    pub fn set_volume(&self, value: Float) {
        *self.controls.volume.lock().unwrap() = value;
    }

    /// Gets the speed of the sound.
    ///
    /// See [`Player::set_speed`] for details on what *speed* means.
    #[inline]
    pub fn speed(&self) -> f32 {
        *self.controls.speed.lock().unwrap()
    }

    /// Changes the play speed of the sound. Does not adjust the samples, only the playback speed.
    ///
    /// # Note:
    /// 1. **Increasing the speed will increase the pitch by the same factor**
    /// - If you set the speed to 0.5 this will halve the frequency of the sound
    ///   lowering its pitch.
    /// - If you set the speed to 2 the frequency will double raising the
    ///   pitch of the sound.
    /// 2. **A change in the speed affects the playback duration inversely**
    /// - If you set the speed to 0.5, playing the sound takes twice as long.
    /// - If you set the speed to 2, playing the sound takes half as long.
    /// 3. **Positions do not depend on the speed**
    /// - [`Player::get_pos`] and [`Player::try_seek`] use positions in the sound
    ///   itself, so they are not scaled by the speed.
    #[inline]
    pub fn set_speed(&self, value: f32) {
        *self.controls.speed.lock().unwrap() = value;
    }

    /// Resumes playback of a paused player.
    ///
    /// No effect if not paused.
    #[inline]
    pub fn play(&self) {
        self.controls.pause.store(false, Ordering::SeqCst);
    }

    // There is no `can_seek()` method as it is impossible to use correctly. Between
    // checking if a source supports seeking and actually seeking the sink can
    // switch to a new source.

    /// Attempts to seek to a given position in the current source.
    ///
    /// `pos` is a position in the source's own timeline and does not depend on the
    /// current playback speed: seeking to 30 seconds lands 30 seconds into the media
    /// whether the speed is 0.5, 1.0 or 2.0. This is the same timeline as
    /// [`Player::get_pos`].
    ///
    /// The target is divided by the current speed before it reaches
    /// [`Speed`](crate::source::Speed), which would otherwise scale it.
    ///
    /// This blocks between 0 and ~5 milliseconds.
    ///
    /// As long as the duration of the source is known, seek is guaranteed to saturate
    /// at the end of the source. For example given a source that reports a total duration
    /// of 42 seconds calling `try_seek()` with 60 seconds as argument will seek to
    /// 42 seconds.
    ///
    /// # Errors
    /// This function will return [`SeekError::NotSupported`] if one of the underlying
    /// sources does not support seeking.
    ///
    /// It will return an error if an implementation ran
    /// into one during the seek.
    ///
    /// When seeking beyond the end of a source this
    /// function might return an error if the duration of the source is not known.
    pub fn try_seek(&self, pos: Duration) -> Result<(), SeekError> {
        let (order, feedback) = SeekOrder::new(pos);
        *self.controls.seek.lock().unwrap() = Some(order);

        if self.sound_count.load(Ordering::Acquire) == 0 {
            // No sound is playing, seek will not be performed
            return Ok(());
        }

        match feedback.recv() {
            Ok(seek_res) => {
                *self.controls.position.lock().unwrap() = pos;
                seek_res
            }
            // The feedback channel closed. Probably another SeekOrder was set
            // invalidating this one and closing the feedback channel
            // ... or the audio thread panicked.
            Err(_) => Ok(()),
        }
    }

    /// Pauses playback of this player.
    ///
    /// No effect if already paused.
    ///
    /// A paused sink can be resumed with `play()`.
    pub fn pause(&self) {
        self.controls.pause.store(true, Ordering::SeqCst);
    }

    /// Gets if a sink is paused
    ///
    /// Players can be paused and resumed using `pause()` and `play()`. This returns `true` if the
    /// sink is paused.
    pub fn is_paused(&self) -> bool {
        self.controls.pause.load(Ordering::SeqCst)
    }

    /// Removes all currently loaded `Source`s from the `Player`, and pauses it.
    ///
    /// See `pause()` for information about pausing a `Player`.
    pub fn clear(&self) {
        let len = self.sound_count.load(Ordering::SeqCst) as u32;
        *self.controls.to_clear.lock().unwrap() = len;
        self.sound_count.store(0, Ordering::Relaxed);
        self.pause();
    }

    /// Skips to the next `Source` in the `Player`
    ///
    /// If there are more `Source`s appended to the `Player` at the time,
    /// it will play the next one. Otherwise, the `Player` will finish as if
    /// it had finished playing a `Source` all the way through.
    pub fn skip_one(&self) {
        let len = self.sound_count.load(Ordering::SeqCst) as u32;
        let mut to_clear = self.controls.to_clear.lock().unwrap();
        if len > *to_clear {
            *to_clear += 1;
        }
        self.sound_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// Stops the sink by emptying the queue.
    #[inline]
    pub fn stop(&self) {
        self.controls.stopped.store(true, Ordering::SeqCst);
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

    /// Returns true if this sink has no more sounds to play.
    #[inline]
    pub fn empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of sounds currently in the queue.
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    pub fn len(&self) -> usize {
        self.sound_count.load(Ordering::Relaxed)
    }

    /// Returns the playback position within the current source.
    ///
    /// The position is measured in the source's own timeline, so it does not depend
    /// on the playback speed. For example, at a speed of `0.5` it advances by half a
    /// second for every second of real time, and changing the speed during playback
    /// does not make it jump.
    ///
    /// After [`Player::try_seek`] this is the position that was sought to.
    ///
    /// The position is read from a [`TrackPosition`](crate::source::TrackPosition) that
    /// sits before [`Speed`](crate::source::Speed) in the adapter chain. See
    /// [`TrackPosition::get_pos`](crate::source::TrackPosition::get_pos) for why the
    /// order matters.
    #[inline]
    pub fn get_pos(&self) -> Duration {
        *self.controls.position.lock().unwrap()
    }
}

impl Drop for Player {
    #[inline]
    fn drop(&mut self) {
        self.queue_tx.set_keep_alive_if_empty(false);

        if !self.detached {
            self.controls.stopped.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use crate::buffer::SamplesBuffer;
    use crate::math::nz;
    use crate::{Player, Source};

    #[test]
    fn test_immediate_length_changes() {
        let (player, mut source) = Player::new();

        player.append(SamplesBuffer::new(nz!(1), nz!(1), vec![2.0, 3.0]));
        player.append(SamplesBuffer::new(nz!(1), nz!(1), vec![1.0, 0.5]));
        assert_eq!(player.len(), 2);
        assert_eq!(source.next(), Some(2.0));

        player.skip_one();
        assert_eq!(player.len(), 1);
        assert_eq!(source.next(), Some(1.0));

        player.clear();
        assert_eq!(player.len(), 0);
    }

    #[test]
    fn test_pause_and_stop() {
        let (player, mut source) = Player::new();

        assert_eq!(source.next(), Some(0.0));
        // TODO (review) How did this test passed before? I might have broken something but
        //      silence source should come first as next source is only polled while previous ends.
        //      Respective test in Queue seem to be ignored (see queue::test::no_delay_when_added()
        //      at src/queue.rs:293).
        let mut source = source.skip_while(|x| *x == 0.0);

        let v = vec![10.0, -10.0, 20.0, -20.0, 30.0, -30.0];

        // Low rate to ensure immediate control.
        player.append(SamplesBuffer::new(nz!(1), nz!(1), v.clone()));
        let mut reference_src = SamplesBuffer::new(nz!(1), nz!(1), v);

        assert_eq!(source.next(), reference_src.next());
        assert_eq!(source.next(), reference_src.next());

        player.pause();

        assert_eq!(source.next(), Some(0.0));

        player.play();

        assert_eq!(source.next(), reference_src.next());
        assert_eq!(source.next(), reference_src.next());

        player.stop();

        assert_eq!(source.next(), Some(0.0));

        assert!(player.empty());
    }

    #[test]
    fn test_stop_and_start() {
        let (player, mut queue_rx) = Player::new();

        let v = vec![10.0, -10.0, 20.0, -20.0, 30.0, -30.0];

        player.append(SamplesBuffer::new(nz!(1), nz!(1), v.clone()));
        let mut src = SamplesBuffer::new(nz!(1), nz!(1), v.clone());

        assert_eq!(queue_rx.next(), src.next());
        assert_eq!(queue_rx.next(), src.next());

        player.stop();

        assert!(player.controls.stopped.load(Ordering::SeqCst));
        assert_eq!(queue_rx.next(), Some(0.0));

        src = SamplesBuffer::new(nz!(1), nz!(1), v.clone());
        player.append(SamplesBuffer::new(nz!(1), nz!(1), v));

        assert!(!player.controls.stopped.load(Ordering::SeqCst));
        // Flush silence
        let mut queue_rx = queue_rx.skip_while(|v| *v == 0.0);

        assert_eq!(queue_rx.next(), src.next());
        assert_eq!(queue_rx.next(), src.next());
    }

    #[test]
    fn test_volume() {
        let (player, mut queue_rx) = Player::new();

        let v = vec![10.0, -10.0, 20.0, -20.0, 30.0, -30.0];

        // High rate to avoid immediate control.
        player.append(SamplesBuffer::new(nz!(2), nz!(44100), v.clone()));
        let src = SamplesBuffer::new(nz!(2), nz!(44100), v.clone());

        let mut src = src.amplify(0.5);
        player.set_volume(0.5);

        for _ in 0..v.len() {
            assert_eq!(queue_rx.next(), src.next());
        }
    }
}
