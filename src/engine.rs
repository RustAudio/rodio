use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::thread::Builder;

use cpal;
use cpal::Device;
use cpal::EventLoop;
use cpal::Sample as CpalSample;
use cpal::UnknownTypeOutputBuffer;
use cpal::StreamId;
use cpal::StreamData;
use dynamic_mixer;
use source::Source;

/// Plays a source with a device until it ends.
///
/// The playing uses a background thread.
pub fn play_raw<S>(device: &Device, source: S)
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
                        engine.events_loop.run(|stream_id, buffer| {
                            audio_callback(&engine, stream_id, buffer);
                        })
                    }
                })
                .ok()
                .map(|jg| jg.thread().clone());

            engine
        };
    }

    start(&ENGINE, device, source);
}

// The internal engine of this library.
//
// Each `Engine` owns a thread that runs in the background and plays the audio.
struct Engine {
    // The events loop which the streams are created with.
    events_loop: EventLoop,

    dynamic_mixers: Mutex<HashMap<StreamId, dynamic_mixer::DynamicMixer<f32>>>,

    // TODO: don't use the device name, as it's slow
    end_points: Mutex<HashMap<String, Weak<dynamic_mixer::DynamicMixerController<f32>>>>,
}

fn audio_callback(engine: &Arc<Engine>, stream_id: StreamId, buffer: StreamData) {
    let mut dynamic_mixers = engine.dynamic_mixers.lock().unwrap();

    let mixer_rx = match dynamic_mixers.get_mut(&stream_id) {
        Some(m) => m,
        None => return,
    };

    match buffer {
        StreamData::Output { buffer: UnknownTypeOutputBuffer::U16(mut buffer) } => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().map(|s| s.to_u16()).unwrap_or(u16::max_value() / 2);
            }
        },
        StreamData::Output { buffer: UnknownTypeOutputBuffer::I16(mut buffer) } => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().map(|s| s.to_i16()).unwrap_or(0i16);
            }
        },
        StreamData::Output { buffer: UnknownTypeOutputBuffer::F32(mut buffer) } => {
            for d in buffer.iter_mut() {
                *d = mixer_rx.next().unwrap_or(0f32);
            }
        },
        StreamData::Input { buffer: _ } => {
            panic!("Can't play an input stream!");
        }
    };
}

// Builds a new sink that targets a given device.
fn start<S>(engine: &Arc<Engine>, device: &Device, source: S)
    where S: Source<Item = f32> + Send + 'static
{
    let mut stream_to_start = None;

    let mixer = {
        let mut end_points = engine.end_points.lock().unwrap();

        match end_points.entry(device.name()) {
            Entry::Vacant(e) => {
                let (mixer, stream) = new_output_stream(engine, device);
                e.insert(Arc::downgrade(&mixer));
                stream_to_start = Some(stream);
                mixer
            },
            Entry::Occupied(mut e) => {
                if let Some(m) = e.get().upgrade() {
                    m.clone()
                } else {
                    let (mixer, stream) = new_output_stream(engine, device);
                    e.insert(Arc::downgrade(&mixer));
                    stream_to_start = Some(stream);
                    mixer
                }
            },
        }
    };

    if let Some(stream) = stream_to_start {
        engine.events_loop.play_stream(stream);
    }

    mixer.add(source);
}

// Adds a new stream to the engine.
// TODO: handle possible errors here
fn new_output_stream(engine: &Arc<Engine>, device: &Device) -> (Arc<dynamic_mixer::DynamicMixerController<f32>>, StreamId) {
    // Determine the format to use for the new stream.
    let format = device
        .supported_output_formats()
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
            if f1.min_sample_rate.0 < 44100 {
                return Some(f2);
            }

            // Privilege outputs with 2 channels for now.
            if f2.channels == 2 && f1.channels != 2 {
                return Some(f2);
            }

            Some(f1)
        })
        .expect("The device doesn't support any format!?")
        .with_max_sample_rate();

    let stream_id = engine.events_loop.build_output_stream(device, &format).unwrap();
    let (mixer_tx, mixer_rx) = {
        dynamic_mixer::mixer::<f32>(format.channels, format.sample_rate.0)
    };

    engine.dynamic_mixers.lock().unwrap().insert(stream_id.clone(), mixer_rx);

    (mixer_tx, stream_id)
}
