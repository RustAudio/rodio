use rodio::source::ChannelVolume;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let input = rodio::source::SineWave::new(440.0);
    let chan_vol = ChannelVolume::new(input, vec![0.01, 0.01, 0.0, 0.0, 0.0, 0.0]);
    sink.append(chan_vol);

    sink.sleep_until_end();
}
