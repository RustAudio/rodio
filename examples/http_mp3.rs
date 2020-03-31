fn main() {
	let device = rodio::default_output_device().unwrap();
	let sink = rodio::Sink::new(&device);

	let url = "https://github.com/RustAudio/rodio/raw/master/examples/music.mp3";
	let request = rodio::SeekableRequest::get(url);
	let buffer = rodio::SeekableBufReader::new(request);
	let source = rodio::Decoder::new(buffer).unwrap();

	sink.append(source);
	sink.sleep_until_end();
}
