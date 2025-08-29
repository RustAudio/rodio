use crate::common::assert_error_traits;
use crate::Source;
use hound::{SampleFormat, WavSpec};
use std::path;
use std::io;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, Clone)]
pub enum ToWavError {
    #[error("Opening file for writing")]
    OpenFile(#[source] Arc<std::io::Error>),
    #[error("Could not create wav writer")]
    Creating(#[source] Arc<hound::Error>),
    #[error("Failed to write samples writer")]
    Writing(#[source] Arc<hound::Error>),
    #[error("Failed to update the wav header")]
    Finishing(#[source] Arc<hound::Error>),
}
assert_error_traits!(ToWavError);

/// Saves Source's output into a wav file. The output samples format is 32-bit
/// float. This function is intended primarily for testing and diagnostics. It can be used to see
/// the output without opening output stream to a real audio device.
///
/// If the file already exists it will be overwritten.
pub fn output_to_wav(
    source: &mut impl Source,
    wav_file: impl AsRef<path::Path>,
) -> Result<(), ToWavError> {
    let file = std::fs::File::create(wav_file)
        .map_err(Arc::new)
        .map_err(ToWavError::OpenFile)?;
    collect_to_wav(source, file)
}

/// Saves Source's output into a writer. The output samples format is 32-bit float. This function
/// is intended primarily for testing and diagnostics. It can be used to see the output without
/// opening output stream to a real audio device.
pub fn collect_to_wav(
    source: &mut impl Source,
    writer: impl io::Write + io::Seek,
) -> Result<(), ToWavError> {
    let format = WavSpec {
        channels: source.channels().get(),
        sample_rate: source.sample_rate().get(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let writer = io::BufWriter::new(writer);
    let mut writer = hound::WavWriter::new(writer, format)
        .map_err(Arc::new)
        .map_err(ToWavError::Creating)?;
    for sample in source {
        writer
            .write_sample(sample)
            .map_err(Arc::new)
            .map_err(ToWavError::Writing)?;
    }
    writer
        .finalize()
        .map_err(Arc::new)
        .map_err(ToWavError::Finishing)?;
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
