use std::{error::Error, sync::Arc};

use rodio::decoder::DecoderBuilder;
use symphonia::{core::codecs::CodecRegistry, default::register_enabled_codecs};

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let mut codec_registry = CodecRegistry::new();
    codec_registry.register_all::<OpusDecoder>();
    register_enabled_codecs(&mut codec_registry);

    let codec_registry_arc = Arc::new(codec_registry);

    let file = std::fs::File::open("assets/music.opus")?;
    let decoder = DecoderBuilder::new()
                    .with_codec_registry(codec_registry_arc)
                    .with_data(file).build()?;
    sink.append(rodio::Decoder::try_from(file)?);

    sink.sleep_until_end();

    Ok(())
}
