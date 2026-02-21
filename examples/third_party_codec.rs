use rodio::decoder::DecoderBuilder;
use std::error::Error;
use symphonia_adapter_fdk_aac::AacDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let sink = rodio::Player::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.m4a")?;
    let len = file.metadata()?.len();
    let decoder = DecoderBuilder::new()
        .with_data(file)
        // Note: the length must be known for Symphonia to properly detect the format for this file
        // This limitation will be removed in Symphonia 0.6
        .with_byte_len(len)
        .with_decoder::<AacDecoder>()
        .build()?;
    sink.append(decoder);

    sink.sleep_until_end();

    Ok(())
}
