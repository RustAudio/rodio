use std::{fs::File, io::BufReader};

fn main() {
	let device = rodio::default_output_device().unwrap();
	let sink = rodio::Sink::new(&device);

	let path = "examples/music.flac";
	let file = File::open(path).unwrap();
	let buffer = BufReader::new(file);
	let source = rodio::Decoder::new(buffer).unwrap();

	sink.append(source);
	sink.sleep_until_end();
}
