macro_rules! add_inner_methods {
    ($name:ident$(<$t:ident$(:$bound:path)?>)?) => {
        impl<S: crate::FixedSource $(,$t$(:$bound)?)?> $name<S $(,$t)?> {
            crate::common::source::add_inner_accessors!{inner}
        }
    };
}

pub(crate) use add_inner_methods;

macro_rules! impl_wrapper {
    ($name:ident$(<$t:ident$(:$bound:path)?>)?) => {
        impl<S: crate::FixedSource $(,$t$(:$bound)?)?> crate::FixedSource for $name<S $(,$t)?> {
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
pub(crate) use impl_wrapper;
