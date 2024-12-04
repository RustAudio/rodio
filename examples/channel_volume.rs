use rodio::source::ChannelVolume;

fn main() {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream().unwrap();
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let input = rodio::source::SineWave::new(440.0);
    let chan_vol = ChannelVolume::new(input, vec![0.01, 0.0, 0.1, 0.1, 0.1, 0.5]);
    sink.append(chan_vol);

    sink.sleep_until_end();
}
