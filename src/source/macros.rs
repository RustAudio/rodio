macro_rules! add_inner_methods {
    ($name:ident$(<$t:ident>)?) => {
        impl<S: crate::Source$(,$t)?> $name<S$(,$t)?> {
            crate::common::source::add_inner_accessors!{inner}
        }
    };
}

macro_rules! impl_wrapper {
    ($name:ident$(<$t:ident>)?) => {
        impl<S: crate::Source$(,$t)?> crate::Source for $name<S$(,$t)?> {
            fn current_span_len(&self) -> Option<usize> {
                self.inner.current_span_len()
            }

            fn channels(&self) -> crate::ChannelCount {
                self.inner.channels()
            }

            fn sample_rate(&self) -> crate::SampleRate {
                self.inner.sample_rate()
            }

            fn total_duration(&self) -> Option<std::time::Duration> {
                self.inner.total_duration()
            }
        }
    };
}

pub(crate) use add_inner_methods;
pub(crate) use impl_wrapper;
