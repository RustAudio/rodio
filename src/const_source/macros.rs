macro_rules! add_inner_methods {
    ($name:ident$(<$t:ident$(:$bound:path)?>)?) => {
        impl<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>$(,$t$(:$bound)?)?> $name<SR, CH, S$(,$t)?> {
            crate::common::source::add_inner_accessors!{inner}
        }
    };
}

pub(crate) use add_inner_methods;

macro_rules! impl_wrapper {
    ($name:ident$(<$t:ident$(:$bound:path)?>)?) => {
        impl<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>$(,$t$(:$bound)?)?> crate::ConstSource<SR, CH>
            for $name<SR, CH, S$(,$t)?>
        {
            fn total_duration(&self) -> Option<std::time::Duration> {
                self.inner.total_duration()
            }
        }
    };
}
pub(crate) use impl_wrapper;
