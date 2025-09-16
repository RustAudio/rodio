use cpal::traits::StreamTrait;
use std::sync::{Arc, Mutex, Weak};

// TODO add more such as one for sources passed to a mixer (should only call
// pause on the downstream PauseHandle when all stream in the mixer have been
// baused
#[derive(Debug, Clone)]
pub(crate) enum PauseControl {
    StreamMixer(StreamMixerControl),
}

impl PauseControl {
    fn pause(&self) {
        match self {
            PauseControl::StreamMixer(stream_mixer_control) => stream_mixer_control.pause(),
        }
    }
    fn unpause(&self) {
        match self {
            PauseControl::StreamMixer(stream_mixer_control) => stream_mixer_control.unpause(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StreamMixerControl {
    pub(crate) stream: Arc<Mutex<Option<Weak<cpal::Stream>>>>,
}

impl StreamMixerControl {
    fn pause(&self) {
        let stream = self.stream.lock().expect("audio thread should not panic");
        let stream = stream.as_ref().expect("should be set just after creation");
        let Some(stream) = stream.upgrade() else {
            return; // stream has been dropped
        };
        stream.pause().unwrap(); // TODO (defer till design done and working) errors
    }
    fn unpause(&self) {
        let stream = self.stream.lock().expect("audio thread should not panic");
        let stream = stream.as_ref().expect("should be set just after creation");
        let Some(stream) = stream.upgrade() else {
            return; // stream has been dropped
        };
        stream.play().unwrap(); // TODO (defer till design done and working) errors
    }
}

impl StreamMixerControl {
    pub(crate) fn set_stream(&self, stream: std::sync::Weak<cpal::Stream>) {
        *self.stream.lock().expect("audio thread should not panic") = Some(stream);
    }
}

#[derive(Debug, Clone)]
pub struct PauseHandle {
    pub(crate) control: Arc<PauseControl>,
}

impl PauseHandle {
    pub(crate) fn pause(&self) {
        self.control.pause();
    }

    pub(crate) fn unpause(&self) {
        self.control.unpause();
    }

    pub(crate) fn new(control: PauseControl) -> Self {
        Self { control: Arc::new(control) }
    }
}
