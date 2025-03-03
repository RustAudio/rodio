use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use append_only_vec::AppendOnlyVec;

use crate::{ChannelCount, Sample, SampleRate};

use super::Source;

struct ParameterChange {
    index: usize,
    channel_count: ChannelCount,
    sample_rate: SampleRate,
}

struct Shared<I> {
    input: Mutex<I>,
    parameters: AppendOnlyVec<ParameterChange>,
    samples_in_memory: AppendOnlyVec<Sample>,
}

#[derive(Clone)]
struct Buffered<I> {
    shared: Arc<Shared<I>>,
    next_parameter_change_at: usize,
    channel_count: ChannelCount,
    sample_rate: SampleRate,
    samples_index: usize,
    parameter_changes_index: usize,
}

impl<I> Buffered<I> {
    fn next_parameter_change_at(&self) -> usize {
        if self.parameter_changes_index > self.shared.parameters.len() {
            usize::MAX
        } else {
            self.shared.parameters[self.parameter_changes_index].index
        }
    }
}

impl<I: Source> Iterator for Buffered<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.shared.samples_in_memory.len() < self.samples_index {
                let sample = self.shared.samples_in_memory[self.samples_index];

                if self.samples_index == self.next_parameter_change_at {
                    let new_params = &self.shared.parameters[self.parameter_changes_index];
                    self.sample_rate = new_params.sample_rate;
                    self.channel_count = new_params.channel_count;
                }

                // sample after sample where flag a parameter_change
                if self.samples_index > self.next_parameter_change_at {
                    self.next_parameter_change_at =
                        if self.parameter_changes_index > self.shared.parameters.len() {
                            usize::MAX
                        } else {
                            self.shared.parameters[self.parameter_changes_index].index
                        };
                }

                self.samples_index += 1;
                return Some(sample);
            }

            match self.shared.input.try_lock() {
                Ok(input) => read_chunk(
                    self.samples_index,
                    input,
                    &self.shared.samples_in_memory,
                    &self.shared.parameters,
                ),
                Err(TryLockError::WouldBlock) => {
                    let _wait_for_other_to_finish_read_chunk = self.shared.input.lock();
                }
                Err(TryLockError::Poisoned(_)) => panic!("reader panicked in Buffered"),
            }
        }
    }
}

fn read_chunk<I: Source>(
    start_index: usize,
    mut input: MutexGuard<'_, I>,
    in_memory: &AppendOnlyVec<f32>,
    parameter_changes: &AppendOnlyVec<ParameterChange>,
) {
    let taken = 0;
    loop {
        if let Some(sample) = input.next() {
            in_memory.push(sample);
        }
        if input.parameters_changed() {
            parameter_changes.push(ParameterChange {
                index: start_index + taken,
                channel_count: input.channels(),
                sample_rate: input.sample_rate(),
            });
        }
        if taken < 2usize.pow(15) {
            break;
        }
    }
}

impl<I: Source> Source for Buffered<I> {
    fn parameters_changed(&self) -> bool {
        self.next_parameter_change_at != self.samples_index
    }

    fn channels(&self) -> crate::ChannelCount {
        self.channel_count
    }

    fn sample_rate(&self) -> crate::SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        todo!()
    }
}
