use crate::{Sample, Source};
use hound::{SampleFormat, WavSpec};
use std::time::Duration;

pub struct LowPass<I>
where
    I: Iterator,
{
    input: I,
    prev: Option<I::Item>,
}

impl<I> LowPass<I>
where
    I: Iterator,
    I::Item: Sample,
{
    #[inline]
    pub fn new(input: I) -> LowPass<I> {
        LowPass { input, prev: None }
    }
}

impl<I> Source for LowPass<I>
where
    I: Source,
    I::Item: Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

impl<I> Iterator for LowPass<I>
where
    I: Iterator,
    I::Item: Sample + Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.next().map(|s| {
            let x = self.prev.map_or(s, |p| (p.saturating_add(s)).amplify(0.5));
            self.prev.replace(s);
            x
        })
    }
}

pub fn output_to_wav<S: Sample>(
    source: Box<dyn Source<Item = S>>,
    wav_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let format = WavSpec {
        channels: source.channels(),
        sample_rate: source.sample_rate(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_file, format)?;
    for sample in source {
        writer.write_sample(sample.to_f32())?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::conversions::SampleRateConverter;
    use crate::source::SineWave;
    use crate::OutputStreamBuilder;
    use crate::Source;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_resample_low_pass() {
        // Generate sine wave.
        let wave = SineWave::new(740.0)
            .amplify(0.1)
            .take_duration(Duration::from_secs(1));

        let rate_in = wave.sample_rate();
        let channels_in = wave.channels();
        let out_freq = 44_100 / 3;
        let output1 = SampleRateConverter::new(
            wave,
            cpal::SampleRate(rate_in),
            cpal::SampleRate(out_freq * 2), 
            channels_in,
        );

        // output_to_wav(Box::new(output1), "resample-orig.wav")
        //     .expect("write wav file");

        let lo_pass = LowPass::new(output1);
        
        let rate_in = lo_pass.sample_rate();
        let output2 = SampleRateConverter::new(
            lo_pass,
            cpal::SampleRate(rate_in),
            cpal::SampleRate(out_freq),
            channels_in,
        );
        output_to_wav(Box::new(output2), "resample-primitive-low-pass.wav")
            .expect("write wav file");
    }

    #[test]
    fn test_resample_biquad_low_pass() {
        let stream_handle = OutputStreamBuilder::open_default_stream().unwrap();
        let mixer = stream_handle.mixer();
        {
            // Generate sine wave.
            let wave = SineWave::new(740.0)
                .amplify(0.1)
                .take_duration(Duration::from_secs(1));

            let rate_in = wave.sample_rate();
            let channels_in = wave.channels();
            let out_freq = 44_100;
            let output1 = SampleRateConverter::new(
                wave,
                cpal::SampleRate(rate_in),
                cpal::SampleRate(out_freq * 2),
                channels_in,
            );

            let lo_pass = LowPass::new(output1);

            let rate_in = lo_pass.sample_rate();
            let output2 = SampleRateConverter::new(
                lo_pass,
                cpal::SampleRate(rate_in),
                cpal::SampleRate(out_freq),
                channels_in,
            );

            mixer.add(output2);
        }

        thread::sleep(Duration::from_millis(1000));
    }
}
