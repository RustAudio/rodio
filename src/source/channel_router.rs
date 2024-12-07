use std::collections::HashMap;

use crate::{Sample, Source};



#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct InputOutputPair(u16, u16);


#[derive(Clone, Debug)]
pub struct ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    input: I,
    channel_mappings: HashMap<InputOutputPair, f32>,
    current_channel: u16,
    input_buffer: Vec<I::Item>,
}

impl<I> ChannelRouter<I> where
    I: Source,
    I::Item: Sample,{

    pub fn new(input: I, channel_count: u16, channel_mappings: HashMap<InputOutputPair, f32>) -> Self {
        Self {
            input,
            channel_mappings,
            current_channel: 0u16,
            input_buffer: vec![<I::Item as Sample>::zero_value(); channel_count.into() ],
        }
    }
}

impl<I> Iterator for ChannelRouter<I> where I: Source, I::Item: Sample {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
