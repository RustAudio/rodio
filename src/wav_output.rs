use crate::common::assert_error_traits;
use crate::Source;
use hound::{SampleFormat, WavSpec};
use std::io::{self, Write};
use std::path;
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
    #[error("Failed to flush all bytes to writer")]
    Flushing(#[source] Arc<std::io::Error>),
}
assert_error_traits!(ToWavError);

/// Saves Source's output into a wav file. The output samples format is 32-bit
/// float. This function is intended primarily for testing and diagnostics. It can be used to see
/// the output without opening output stream to a real audio device.
///
/// If the file already exists it will be overwritten.
///
/// # Note
/// This is a convenience wrapper around `wav_to_writer`
pub fn wav_to_file(
    source: impl Source,
    wav_file: impl AsRef<path::Path>,
) -> Result<(), ToWavError> {
    let mut file = std::fs::File::create(wav_file)
        .map_err(Arc::new)
        .map_err(ToWavError::OpenFile)?;
    wav_to_writer(source, &mut file)
}

/// Saves Source's output into a writer. The output samples format is 32-bit float. This function
/// is intended primarily for testing and diagnostics. It can be used to see the output without
/// opening output stream to a real audio device.
///
/// # Example
/// ```rust
/// # use rodio::static_buffer::StaticSamplesBuffer;
/// # use rodio::collect_to_wav;
/// # const SAMPLES: [rodio::Sample; 5] = [0.0, 1.0, 2.0, 3.0, 4.0];
/// # let source = StaticSamplesBuffer::new(
/// #     1.try_into().unwrap(),
/// #     1.try_into().unwrap(),
/// #     &SAMPLES
/// # );
/// let mut writer = std::io::Cursor::new(Vec::new());
/// wav_to_writer(source, &mut writer)?;
/// let wav_bytes: Vec<u8> = writer.into_inner();
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn wav_to_writer(
    source: impl Source,
    writer: &mut (impl io::Write + io::Seek),
) -> Result<(), ToWavError> {
    let format = WavSpec {
        channels: source.channels().get(),
        sample_rate: source.sample_rate().get(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = io::BufWriter::new(writer);
    {
        let mut writer = hound::WavWriter::new(&mut writer, format)
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
    }
    writer
        .flush()
        .map_err(Arc::new)
        .map_err(ToWavError::Flushing)?;
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
