use crate::{Sample, Source};
use hound::{SampleFormat, WavSpec};
use std::path;

/// This procedure saves Source's output into a wav file. The output samples format is 32-bit float.
/// This function is intended primarily for testing and diagnostics. It can be used to see
/// the output without opening output stream to a real audio device.
pub fn output_to_wav<S: Sample>(
    source: &mut impl Source<Item = S>,
    wav_file: impl AsRef<path::Path>,
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
    use super::output_to_wav;
    use crate::Source;
    use std::time::Duration;

    #[test]
    fn test_output_to_wav() {
        let mut new_source = crate::source::SineWave::new(745.0)
            .amplify(0.1)
            .take_duration(Duration::from_secs(1));
        output_to_wav(&mut new_source, &"target/tmp/save-to-wav-test.wav").unwrap();
    }
}
