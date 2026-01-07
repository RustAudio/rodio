use std::{error::Error, sync::Arc};

use rodio::decoder::DecoderBuilder;
use symphonia::{core::codecs::CodecRegistry, default::register_enabled_codecs};
use symphonia_adapter_libopus::OpusDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let sink = rodio::Player::connect_new(stream_handle.mixer());

    let mut codec_registry = CodecRegistry::new();
    codec_registry.register_all::<OpusDecoder>();
    register_enabled_codecs(&mut codec_registry);

    let codec_registry_arc = Arc::new(codec_registry);

    let file = std::fs::File::open("../assets/music.opus")?;
    let decoder = DecoderBuilder::new()
        .with_codec_registry(codec_registry_arc)
        .with_data(file)
        .build()?;
    sink.append(decoder);

    sink.sleep_until_end();

    Ok(())
}
