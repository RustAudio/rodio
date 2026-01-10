//! This is an attempt at measuring jitter. If it works it _might_ be worth
//! translating it into its own crate. Should look to divan for inspiration.
//! We might also move parts into rodio under a `jitter-bench` feature.
//!
//! To make this work we need to take the place of cpal. Luckily that is super
//! easy, we just need something that takes a Source and consumes.
//!
//! We measure the timing between each sample. To prevent the timing overhead
//! from influencing the measurements too much we directly fetch the processors
//! timing register. This means zero syscalls or conversions are done during
//! measurements.
//!
//! Though this does mean this only works on x86 (until someone implements the
//! same thing for aarch64 for example).

use std::hint::black_box;
use std::sync::Mutex;
use std::time::Duration;

use hdrhistogram::Histogram;
use rodio::Source;

fn panic_if_debug() {
    #[cfg(debug_assertions)]
    panic!("This is a benchmark it *must* be build with optimizations!");
}

#[cfg(target_arch = "x86_64")]
fn measure(mut source: impl Source) {
    use std::time::Instant;
    use thread_priority::ThreadExt;

    panic_if_debug();

    static CYCLES: Mutex<[u64; 100_000]> = Mutex::new([0u64; 100_000]);
    let mut cycles = CYCLES.lock().unwrap();

    if !fastant::is_tsc_available() {
        panic!("Platform does not support Time Stamp Counter or it is not stable")
    }

    // TODO warm up CPU so we have a bigger change that the frequency stays the
    // same

    // Try and keep pre-emption to an absolute minimum
    let og_priority = std::thread::current().get_priority().unwrap();
    thread_priority::ThreadPriority::Max
        .set_for_current()
        .expect("Could not set thread priority to max, are you running as super user?");

    // TODO replace with FIFO sched.
    //I don't think you can do that, but instead, you can call sched_setscheduler() to give your process SCHED_FIFO scheduling policy and a suitable (nonzero) priority. That makes it a real-time task which cannot be interrupted except by another higher-priority real-time task (of which there are probably none).


    let started = Instant::now();
    for cycle_count in cycles.iter_mut() {
        // We do not use any memory fencing that means this instruction can be
        // reorderd slightly. That does not matter for finding the max/min
        // latency
        *cycle_count = unsafe { core::arch::x86_64::_rdtsc() };
        black_box(source.next());
    }
    og_priority.set_for_current().unwrap();

    print_statistics(&*cycles, started.elapsed());
}

fn print_statistics(cycles: &[u64], elapsed: Duration) {
    let per_sample: Vec<u64> = cycles
        .windows(2)
        .take_while(|w| w.iter().all(|c| *c != 0))
        .map(|w| w[1] - w[0])
        .collect();
    let average = per_sample.iter().sum::<u64>() / per_sample.len() as u64;
    let min = per_sample.iter().min().copied().unwrap();
    let max = per_sample
        .iter()
        // TODO detect time pre-empted.... if we where...
        .max()
        .copied()
        .unwrap();
    let median = {
        let mut per_sample = per_sample.clone();
        per_sample.sort();
        per_sample[per_sample.len() / 2]
    };

    let mut histogram: Histogram<u64> = Histogram::new_with_max(max, 5).unwrap();
    for sample in &per_sample {
        histogram.record(*sample).unwrap();
    }

    let total_cycles = cycles.last().unwrap() - cycles.first().unwrap();
    assert_eq!(
        total_cycles as f64 as u64, total_cycles,
        "do not lose precision"
    );

    let dur_median = elapsed.mul_f64(median as f64 / total_cycles as f64);
    let dur_average = elapsed.mul_f64(average as f64 / total_cycles as f64);
    let dur_min = elapsed.mul_f64(min as f64 / total_cycles as f64);
    let dur_max = elapsed.mul_f64(max as f64 / total_cycles as f64);

    println!("took {elapsed:?}");
    println!("----------------------------------");
    println!("median \t  {dur_median:?}\t{median} cycles");
    println!("average\t  {dur_average:?}\t{average} cycles");
    println!("min    \t  {dur_min:?}\t{min} cycles");
    println!("max    \t  {dur_max:?}\t{max} cycles");
    println!("5% lows\t  {}", histogram.value_at_quantile(0.95));
    println!("1% lows\t  {}", histogram.value_at_quantile(0.99));
    println!("0.1% lows\t {}", histogram.value_at_quantile(0.999));
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {
    use std::process::exit;

    eprintln!("Jitter benchmark is only supported on x86_64");
    std::process::exit(-1)
}

#[cfg(target_arch = "x86_64")]
fn main() {
    use rodio::nz;
    use rodio::source::noise::WhiteGaussian;
    use rodio::source::{AutomaticGainControlSettings, LimitSettings, SineWave};

    println!("\nSine wave");
    let source = SineWave::new(440.0);
    measure(source);

    println!("\nWhite Noise");
    let source = WhiteGaussian::new(nz!(44100));
    measure(source);

    println!("\nWhite Noise with AGC, amplify and limit");
    let source = WhiteGaussian::new(nz!(44100))
        .automatic_gain_control(AutomaticGainControlSettings::default())
        .amplify(0.5)
        .limit(LimitSettings::dynamic_content());
    measure(source);

    println!("\nWav file");
    let file = std::fs::File::open("assets/music.wav").unwrap();
    let source = rodio::Decoder::try_from(file).unwrap();
    measure(source);
}
