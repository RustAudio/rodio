use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() {
    let stream_handle = rodio::OutputStreamBuilder::try_default_stream()
        .expect("open default audio stream");
    let mixer = stream_handle.mixer();

    {
        let file = std::fs::File::open("assets/beep.wav").unwrap();
        let sink = rodio::play(&mixer, BufReader::new(file)).unwrap();
        sink.set_volume(0.2);
        println!("Started beep1");
        thread::sleep(Duration::from_millis(1500));
        sink.detach();
    }
    {
        let file = std::fs::File::open("assets/beep.wav").unwrap();
        let sink = rodio::play(&mixer, BufReader::new(file)).unwrap();
        sink.set_volume(0.2);
        println!("Started beep1");
        thread::sleep(Duration::from_millis(1500));
        sink.detach();
    }

    // let file = std::fs::File::open("assets/beep2.wav").unwrap();
    // let beep2 = stream_handle.play_once(BufReader::new(file)).unwrap();
    // beep2.set_volume(0.3);
    // beep2.detach();
    // println!("Started beep2");
    //
    // thread::sleep(Duration::from_millis(1500));
    // let file = std::fs::File::open("assets/beep3.ogg").unwrap();
    // let beep3 = stream_handle.play_once(file).unwrap();
    // beep3.set_volume(0.2);
    // println!("Started beep3");
    //
    // thread::sleep(Duration::from_millis(1500));
    // drop(beep1);
    // println!("Stopped beep1");
    //
    // thread::sleep(Duration::from_millis(1500));
    // drop(beep3);
    // println!("Stopped beep3");
    //
    // thread::sleep(Duration::from_millis(1500));
}
