use cpal::traits::{DeviceTrait, StreamTrait};
use device::CpalDeviceExt;
use dynamic_mixer::DynamicMixerController;
use source::Source;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct DeviceMixer {
    // TODO: don't use the device name as it's slow
    /// Device name -> (mixer, stream)
    mixers: HashMap<String, (Arc<DynamicMixerController<f32>>, cpal::Stream)>,
}

impl DeviceMixer {
    pub(crate) fn play<S>(&mut self, device: &cpal::Device, source: S)
    where
        S: Source<Item = f32> + Send + 'static,
    {
        let device_name = device.name().expect("No device name");

        let (ref mut mixer, _) = self.mixers.entry(device_name).or_insert_with(|| {
            let (mixer, stream) = device.new_output_stream();
            stream.play().expect("play");
            (mixer, stream)
        });

        mixer.add(source);
    }
}
