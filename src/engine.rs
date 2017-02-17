use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::thread::Builder;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;

use futures::stream::Stream;
use futures::task;
use futures::task::Executor;
use futures::task::Run;

use cpal;
use cpal::UnknownTypeBuffer;
use cpal::EventLoop;
use cpal::Voice;
use cpal::Endpoint;
use conversions::Sample;
use dynamic_mixer;
use source::Source;

/// Plays a source to an end point until it ends.
///
/// The playing uses a background thread.
pub fn play_raw<S>(endpoint: &Endpoint, source: S)
    where S: Source<Item = f32> + Send + 'static
{
    lazy_static! {
        static ref ENGINE: Engine = {
            let events_loop = Arc::new(EventLoop::new());

            // We ignore errors when creating the background thread.
            // The user won't get any audio, but that's better than a panic.
            Builder::new()
                .name("rodio audio processing".to_string())
                .spawn({
                    let events_loop = events_loop.clone();
                    move || events_loop.run()
                })
                .ok()
                .map(|jg| jg.thread().clone());

            Engine {
                events_loop: events_loop,
                end_points: Mutex::new(HashMap::with_capacity(1)),
            }
        };
    }

    ENGINE.start(endpoint, source);
}

// The internal engine of this library.
//
// Each `Engine` owns a thread that runs in the background and plays the audio.
struct Engine {
    // The events loop which the voices are created with.
    events_loop: Arc<EventLoop>,

    // TODO: don't use the endpoint name, as it's slow
    end_points: Mutex<HashMap<String, Weak<dynamic_mixer::DynamicMixerController<f32>>>>,
}

impl Engine {
    // Builds a new sink that targets a given endpoint.
    fn start<S>(&self, endpoint: &Endpoint, source: S)
        where S: Source<Item = f32> + Send + 'static
    {
        let mut voice_to_start = None;

        let mixer = {
            let mut end_points = self.end_points.lock().unwrap();

            match end_points.entry(endpoint.get_name()) {
                Entry::Vacant(e) => {
                    let (mixer, voice) = new_voice(endpoint, &self.events_loop);
                    e.insert(Arc::downgrade(&mixer));
                    voice_to_start = Some(voice);
                    mixer
                },
                Entry::Occupied(mut e) => {
                    if let Some(m) = e.get().upgrade() {
                        m.clone()
                    } else {
                        let (mixer, voice) = new_voice(endpoint, &self.events_loop);
                        e.insert(Arc::downgrade(&mixer));
                        voice_to_start = Some(voice);
                        mixer
                    }
                },
            }
        };

        mixer.add(source);

        if let Some(mut voice) = voice_to_start {
            voice.play();
        }
    }
}

// TODO: handle possible errors here
fn new_voice(endpoint: &Endpoint, events_loop: &Arc<EventLoop>)
             -> (Arc<dynamic_mixer::DynamicMixerController<f32>>, Voice) {
    // Determine the format to use for the new voice.
    let format = endpoint.get_supported_formats_list()
        .unwrap()
        .fold(None, |f1, f2| {
            if f1.is_none() {
                return Some(f2);
            }

            let f1 = f1.unwrap();

            // We privilege f32 formats to avoid a conversion.
            if f2.data_type == cpal::SampleFormat::F32 &&
                f1.data_type != cpal::SampleFormat::F32 {
                return Some(f2);
            }

            // Do not go below 44100 if possible.
            if f1.samples_rate.0 < 44100 {
                return Some(f2);
            }

            // Priviledge outputs with 2 channels for now.
            if f2.channels.len() == 2 && f1.channels.len() != 2 {
                return Some(f2);
            }

            Some(f1)
        })
        .expect("The endpoint doesn't support any format!?");

    let (voice, stream) = Voice::new(&endpoint, &format, events_loop)
        .unwrap();

    let (mixer_tx, mut mixer_rx) = {
        dynamic_mixer::mixer::<f32>(format.channels.len() as u16, format.samples_rate.0)
    };

    let future_to_exec = stream.for_each(move |mut buffer| -> Result<_, ()> {
        match buffer {
            UnknownTypeBuffer::U16(ref mut buffer) => {
                for (o, i) in buffer.iter_mut().zip(mixer_rx.by_ref()) {
                    *o = i.to_u16();
                }
            }
            UnknownTypeBuffer::I16(ref mut buffer) => {
                for (o, i) in buffer.iter_mut().zip(mixer_rx.by_ref()) {
                    *o = i.to_i16();
                }
            }
            UnknownTypeBuffer::F32(ref mut buffer) => {
                for (o, i) in buffer.iter_mut().zip(mixer_rx.by_ref()) {
                    *o = i;
                }
            }
        };

        Ok(())
    });

    {
        struct MyExecutor;
        impl Executor for MyExecutor {
            fn execute(&self, r: Run) {
                r.run();
            }
        }
        task::spawn(future_to_exec).execute(Arc::new(MyExecutor));
    }

    (mixer_tx, voice)
}
