use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::thread::Builder;

use conversions::Sample;
use cpal;
use cpal::Endpoint;
use cpal::EventLoop;
use cpal::UnknownTypeBuffer;
use cpal::VoiceId;
use dynamic_mixer;
use source::Source;

/// Plays a source to an end point until it ends.
///
/// The playing uses a background thread.
pub fn play_raw<S>(endpoint: &Endpoint, source: S)
    where S: Source<Item = f32> + Send + 'static
{
    lazy_static! {
        static ref ENGINE: Arc<Engine> = {
            let engine = Arc::new(Engine {
                events_loop: EventLoop::new(),
                dynamic_mixers: Mutex::new(HashMap::with_capacity(1)),
                end_points: Mutex::new(HashMap::with_capacity(1)),
            });

            // We ignore errors when creating the background thread.
            // The user won't get any audio, but that's better than a panic.
            Builder::new()
                .name("rodio audio processing".to_string())
                .spawn({
                    let engine = engine.clone();
                    move || {
                        engine.events_loop.run(|voice_id, buffer| {
                            audio_callback(&engine, voice_id, buffer);
                        })
                    }
                })
                .ok()
                .map(|jg| jg.thread().clone());

            engine
        };
    }

    start(&ENGINE, endpoint, source);
}

// The internal engine of this library.
//
// Each `Engine` owns a thread that runs in the background and plays the audio.
struct Engine {
    // The events loop which the voices are created with.
    events_loop: EventLoop,

    dynamic_mixers: Mutex<HashMap<VoiceId, dynamic_mixer::DynamicMixer<f32>>>,

    // TODO: don't use the endpoint name, as it's slow
    end_points: Mutex<HashMap<String, Weak<dynamic_mixer::DynamicMixerController<f32>>>>,
}

fn audio_callback(engine: &Arc<Engine>, voice_id: VoiceId, mut buffer: UnknownTypeBuffer) {
    let mut dynamic_mixers = engine.dynamic_mixers.lock().unwrap();

    let mixer_rx = match dynamic_mixers.get_mut(&voice_id) {
        Some(m) => m,
        None => return,
    };

    match buffer {
        UnknownTypeBuffer::U16(ref mut buffer) => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().map(|s| s.to_u16()).unwrap_or(0u16);
            }
        },
        UnknownTypeBuffer::I16(ref mut buffer) => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().map(|s| s.to_i16()).unwrap_or(0i16);
            }
        },
        UnknownTypeBuffer::F32(ref mut buffer) => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().unwrap_or(0f32);
            }
        },
    };
}

// Builds a new sink that targets a given endpoint.
fn start<S>(engine: &Arc<Engine>, endpoint: &Endpoint, source: S)
    where S: Source<Item = f32> + Send + 'static
{
    let mut voice_to_start = None;

    let mixer = {
        let mut end_points = engine.end_points.lock().unwrap();

        match end_points.entry(endpoint.name()) {
            Entry::Vacant(e) => {
                let (mixer, voice) = new_voice(engine, endpoint);
                e.insert(Arc::downgrade(&mixer));
                voice_to_start = Some(voice);
                mixer
            },
            Entry::Occupied(mut e) => {
                if let Some(m) = e.get().upgrade() {
                    m.clone()
                } else {
                    let (mixer, voice) = new_voice(engine, endpoint);
                    e.insert(Arc::downgrade(&mixer));
                    voice_to_start = Some(voice);
                    mixer
                }
            },
        }
    };

    if let Some(voice) = voice_to_start {
        engine.events_loop.play(voice);
    }

    mixer.add(source);
}

// Adds a new voice to the engine.
// TODO: handle possible errors here
fn new_voice(engine: &Arc<Engine>, endpoint: &Endpoint) -> (Arc<dynamic_mixer::DynamicMixerController<f32>>, VoiceId) {
    // Determine the format to use for the new voice.
    let format = endpoint
        .supported_formats()
        .unwrap()
        .fold(None, |f1, f2| {
            if f1.is_none() {
                return Some(f2);
            }

            let f1 = f1.unwrap();

            // We privilege f32 formats to avoid a conversion.
            if f2.data_type == cpal::SampleFormat::F32 && f1.data_type != cpal::SampleFormat::F32 {
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

    let voice_id = engine.events_loop.build_voice(endpoint, &format).unwrap();
    let (mixer_tx, mixer_rx) = {
        dynamic_mixer::mixer::<f32>(format.channels.len() as u16, format.samples_rate.0)
    };

    engine.dynamic_mixers.lock().unwrap().insert(voice_id.clone(), mixer_rx);

    (mixer_tx, voice_id)
}
