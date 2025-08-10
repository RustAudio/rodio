use crate::Source;
use hound::{SampleFormat, WavSpec};
use std::path;

/// This procedure saves Source's output into a wav file. The output samples format is 32-bit float.
/// This function is intended primarily for testing and diagnostics. It can be used to see
/// the output without opening output stream to a real audio device.
pub fn output_to_wav(
    source: &mut impl Source,
    wav_file: impl AsRef<path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let format = WavSpec {
        channels: source.channels().get(),
        sample_rate: source.sample_rate().get(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_file, format)?;
    for sample in source {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::output_to_wav;
    use crate::Source;
    use std::io::BufReader;
    use std::time::Duration;

    #[test]
    fn test_output_to_wav() {
        let make_source = || {
            crate::source::SineWave::new(745.0)
                .amplify(0.1)
                .take_duration(Duration::from_secs(1))
        };
        let wav_file_path = "target/tmp/save-to-wav-test.wav";
        output_to_wav(&mut make_source(), wav_file_path).expect("output file can be written");

        let file = std::fs::File::open(wav_file_path).expect("output file can be opened");
        // Not using crate::Decoder bcause it is limited to i16 samples.
        let mut reader =
            hound::WavReader::new(BufReader::new(file)).expect("wav file can be read back");
        let reference = make_source();
        assert_eq!(reference.sample_rate().get(), reader.spec().sample_rate);
        assert_eq!(reference.channels().get(), reader.spec().channels);

        let actual_samples: Vec<f32> = reader.samples::<f32>().map(|x| x.unwrap()).collect();
        let expected_samples: Vec<f32> = reference.collect();
        assert!(
            expected_samples == actual_samples,
            "wav samples do not match the source"
        );
    }
}
