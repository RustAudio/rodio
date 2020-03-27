use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::thread::Builder;

use cpal::traits::{DeviceTrait, EventLoopTrait, HostTrait};
use cpal::Device;
use cpal::EventLoop;
use cpal::Sample as CpalSample;
use cpal::StreamData;
use cpal::StreamId;
use cpal::UnknownTypeOutputBuffer;
use dynamic_mixer;
use source::Source;

/// Plays a source with a device until it ends.
///
/// The playing uses a background thread.
pub fn play_raw<S>(device: &Device, source: S)
where
    S: Source<Item = f32> + Send + 'static,
{
    lazy_static! {
        static ref ENGINE: Arc<Engine> = {
            let engine = Arc::new(Engine {
                events_loop: cpal::default_host().event_loop(),
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
                            if let Ok(buf) = buffer {
                                audio_callback(&engine, stream_id, buf);
                            }
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
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::U16(mut buffer),
        } => for d in buffer.iter_mut() {
            *d = mixer_rx
                .next()
                .map(|s| s.to_u16())
                .unwrap_or(u16::max_value() / 2);
        },
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::I16(mut buffer),
        } => for d in buffer.iter_mut() {
            *d = mixer_rx.next().map(|s| s.to_i16()).unwrap_or(0i16);
        },
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::F32(mut buffer),
        } => for d in buffer.iter_mut() {
            *d = mixer_rx.next().unwrap_or(0f32);
        },
        StreamData::Input { .. } => {
            panic!("Can't play an input stream!");
        },
    };
}

// Builds a new sink that targets a given device.
fn start<S>(engine: &Arc<Engine>, device: &Device, source: S)
where
    S: Source<Item = f32> + Send + 'static,
{
    let mut stream_to_start = None;

    let mixer = if let Ok(device_name) = device.name() {
        let mut end_points = engine.end_points.lock().unwrap();

        match end_points.entry(device_name) {
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
    } else {
        let (mixer, stream) = new_output_stream(engine, device);
        stream_to_start = Some(stream);
        mixer
    };

    if let Some(stream) = stream_to_start {
        engine.events_loop.play_stream(stream).expect("play_stream failed");
    }

    mixer.add(source);
}

// Adds a new stream to the engine.
fn new_output_stream(
    engine: &Arc<Engine>,
    device: &Device,
) -> (Arc<dynamic_mixer::DynamicMixerController<f32>>, StreamId) {
    let (format, stream_id) = {
        // Determine the format to use for the new stream.
        let default_format = device
            .default_output_format()
            .expect("The device doesn't support any format!?");

        match engine
            .events_loop
            .build_output_stream(device, &default_format)
        {
            Ok(sid) => (default_format, sid),
            Err(err) => find_working_output_stream(engine, device)
                .ok_or(err)
                .expect("build_output_stream failed with all supported formats"),
        }
    };

    let (mixer_tx, mixer_rx) = dynamic_mixer::mixer::<f32>(format.channels, format.sample_rate.0);

    engine
        .dynamic_mixers
        .lock()
        .unwrap()
        .insert(stream_id.clone(), mixer_rx);

    (mixer_tx, stream_id)
}

/// Search through all the supported formats trying to find one that
/// will `build_output_stream` successfully.
fn find_working_output_stream(
    engine: &Arc<Engine>,
    device: &Device,
) -> Option<(cpal::Format, cpal::StreamId)> {
    const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);

    let mut supported: Vec<_> = device
        .supported_output_formats()
        .expect("No supported output formats")
        .collect();
    supported.sort_by(|a, b| b.cmp_default_heuristics(a));

    supported
        .into_iter()
        .flat_map(|sf| {
            let max_rate = sf.max_sample_rate;
            let min_rate = sf.min_sample_rate;
            let mut formats = vec![sf.clone().with_max_sample_rate()];
            if HZ_44100 < max_rate && HZ_44100 > min_rate {
                formats.push(sf.clone().with_sample_rate(HZ_44100))
            }
            formats.push(sf.with_sample_rate(min_rate));
            formats
        })
        .filter_map(|format| {
            engine
                .events_loop
                .build_output_stream(device, &format)
                .ok()
                .map(|stream| (format, stream))
        })
        .next()
}

trait SupportedFormatExt {
    fn with_sample_rate(self, sample_rate: cpal::SampleRate) -> cpal::Format;
}
impl SupportedFormatExt for cpal::SupportedFormat {
    fn with_sample_rate(self, sample_rate: cpal::SampleRate) -> cpal::Format {
        let Self {
            channels,
            data_type,
            ..
        } = self;
        cpal::Format {
            channels,
            sample_rate,
            data_type,
        }
    }
}
