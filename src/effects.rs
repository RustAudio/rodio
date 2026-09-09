//! All the effects that can be applied to rodio sources. To apply an effect to
//! a source use the method on the source directly. You should only need this
//! module for effects settings and to specify the full type of the source.

/// Amplify effect
pub mod amplify;

// we can only get the structure: effects::effect::source_type::Struct with a macro
// so we re-export the structs here to get the nicer structure:
// effects::source_type::Struct;

pub mod fixed_source {
    //! Effects that work on fixed sources
    pub use super::amplify::fixed_source::Amplify;
}
pub mod const_source {
    //! Effects that work on const sources
    pub use super::amplify::const_source::Amplify;
}
pub mod dynamic_source {
    //! Effects that work on dynamic sources
    pub use super::amplify::dynamic_source::Amplify;
}

/// Write the minimal Rust needed to define the needed source implementations
/// for a pure effect. A pure effect is one which does not create or modify
/// spans. Start with `supports_dynamic_source` on a single line to generate an
/// implementation for Dynamic-, Fixed- and ConstSource. Leave that line out to
/// generate only implementations for Fixed- and ConstSource.
///
/// For example usage see src/effects/amplify.rs
macro_rules! pure_effect {
    (
    supports_dynamic_source
    #[$struct_doc:meta]
    struct $name:ident$(<$t:ident$(:$bound:path)?>)? {
        $($field:ident: $field_ty:ty,)*
    }
    // like `struct` above the `fn`, `&mut` and `-> Option<Sample>` are just there
    // to make the macro input seem regular rust code
    fn next(&mut $self:ident) -> Option<Sample> $body:block
    fn new$(<$new_generic:ident : $new_bound:path>)?($($factory_args:tt)*) -> $factory_name:ident<Self> $factory_body:block
    // m stands for method
    $($(#[$m_meta:meta])* $m_vis:vis fn $m_name:ident($($args:tt)*) $(-> $m_ret:ty)? $m_body:block)*
    ) => {
        pub(crate) mod dynamic_source {
            #[allow(unused)]
            use super::*;
            #[derive(Clone)]
            #[$struct_doc]
            pub struct $name<S: crate::DynamicSource$(,$t$(:$bound)?)?> {
                pub(crate) inner: S,
                $(pub(crate) $field: $field_ty),*
            }

            crate::source::macros::add_inner_methods!{$name$(<$t$(:$bound)?>)?}
            crate::source::macros::impl_wrapper!{$name$(<$t$(:$bound)?>)?}
        }

        impl<S: crate::Source$(,$t$(:$bound)?)?> dynamic_source::$name<S$(,$t)?> {
            #[must_use]
            pub(crate) fn new($($factory_args)*) -> dynamic_source::$name<S$(,$t)?> {
                $factory_body
            }
            $($(#[$m_meta])* $m_vis fn $m_name($($args)*) $(-> $m_ret)? $m_body)*
        }

        impl<S: crate::Source$(,$t$(:$bound)?)?> Iterator for dynamic_source::$name<S$(,$t)?> {
            type Item = crate::Sample;

            fn next(&mut $self) -> Option<Self::Item> {
                $body
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.inner.size_hint()
            }
        }

        impl<S: crate::Source$(,$t$(:$bound)?)?> ExactSizeIterator for dynamic_source::$name<S$(,$t)?> where S: ExactSizeIterator {}

        crate::effects::inner!{
            #[$struct_doc]
            struct $name$(<$t$(:$bound)?>)? {
                $($field: $field_ty,)*
            }
            fn next(&mut $self) -> Option<Sample> $body
            fn new($($factory_args)*) -> $factory_name<Self> $factory_body
            $($(#[$m_meta])* $m_vis fn $m_name($($args)*) $(-> $m_ret)? $m_body)*
        }
    };

    (
    #[$struct_doc:meta]
    struct $name:ident$(<$t:ident$(:$bound:path)?>)? {
        $($field:ident: $field_ty:ty,)*
    }
    // like `struct` above the `fn`, `&mut` and `-> Option<Sample>` are just there
    // to make the macro input seem regular rust code
    fn next(&mut $self:ident) -> Option<Sample> $body:block
    fn new$(<$new_generic:ident : $new_bound:path>)?($($factory_args:tt)*)
    -> $factory_name:ident<Self> $factory_body:block
    // m stands for method
    $($(#[$m_meta:meta])* $m_vis:vis fn $m_name:ident($($args:tt)*) $(-> $m_ret:ty)? $m_body:block)*
    ) => {
        crate::effects::inner!{
            struct $name$(<$t$(:$bound)?>)? {
                $($field: $field_ty,)*
            }
            fn next(&mut $self) -> Option<Sample> $body
            fn new$(<$new_generic: $new_bound>)?($($factory_args)*)
                -> $factory_name<Self> $factory_body
            $($(#[$m_meta])* $m_vis fn $m_name($($args)*) $(-> $m_ret)? $m_body)*
        }
    }
}

macro_rules! inner {
(
    #[$struct_doc:meta]
    struct $name:ident$(<$t:ident$(:$bound:path)?>)? {
        $($field:ident: $field_ty:ty,)*
    }
    // like `struct` above the `fn`, `&mut` and `-> Option<Sample>` are just there
    // to make the macro input seem regular rust code
    fn next(&mut $self:ident) -> Option<Sample> $body:block
    fn new$(<$new_generic:ident: $new_bound:path>)?($($factory_args:tt)*) -> $factory_name:ident<Self> $factory_body:block
    // m stands for method
    $($(#[$m_meta:meta])* $m_vis:vis fn $m_name:ident($($args:tt)*) $(-> $m_ret:ty)? $m_body:block)*
    ) =>  {
        pub(crate) mod fixed_source {
            #[allow(unused)]
            use super::*;

            #[derive(Clone)]
            #[$struct_doc]
            pub struct $name<S: crate::FixedSource$(,$t$(:$bound)?)?> {
                pub(crate) inner: S,
                $(pub(crate) $field: $field_ty),*
            }

            crate::fixed_source::macros::add_inner_methods!{$name$(<$t$(:$bound)?>)?}
            crate::fixed_source::macros::impl_wrapper!{$name$(<$t$(:$bound)?>)?}
        }

        impl<S: crate::FixedSource $(,$t$(:$bound)?)?> fixed_source::$name<S$(,$t)?> {
            #[must_use]
            pub(crate) fn new$(<$new_generic: $new_bound>)?($($factory_args)*)
                -> fixed_source::$name<S$(,$t)?> {
                $factory_body
            }
            $($(#[$m_meta])* $m_vis fn $m_name($($args)*) $(-> $m_ret)? $m_body)*
        }

        impl<S: crate::FixedSource$(,$t$(:$bound)?)?>
            ExactSizeIterator for fixed_source::$name<S$(,$t)?>
                where S: ExactSizeIterator {}

        impl<S: crate::FixedSource$(,$t$(:$bound)?)?>
            Iterator for fixed_source::$name<S$(,$t)?> {

            type Item = crate::Sample;

            fn next(&mut $self) -> Option<Self::Item> {
                $body
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.inner.size_hint()
            }
        }

        pub(crate) mod const_source {
            #[allow(unused)]
            use super::*;

            #[derive(Clone)]
            #[$struct_doc]
            pub struct $name<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>
                $(,$t$(:$bound)?)?> {
                pub(crate) inner: S,
                $(pub(crate) $field: $field_ty),*
            }

            crate::const_source::macros::add_inner_methods!{$name$(<$t$(:$bound)?>)?}
            crate::const_source::macros::impl_wrapper!{$name$(<$t$(:$bound)?>)?}
        }

        impl<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>$(,$t$(:$bound)?)?>
            const_source::$name<SR, CH, S$(,$t)?> {

            #[must_use]
            pub(crate) fn new$(<$new_generic: $new_bound>)?($($factory_args)*)
                -> const_source::$name<SR, CH, S$(,$t)?> {
                $factory_body
            }
            $($(#[$m_meta])* $m_vis fn $m_name($($args)*) $(-> $m_ret)? $m_body)*
        }


        impl<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>$(,$t$(:$bound)?)?>
            ExactSizeIterator for const_source::$name<SR, CH, S$(,$t)?>
                where S: ExactSizeIterator {}

        impl<const SR: u32, const CH: u16, S: crate::ConstSource<SR, CH>$(,$t$(:$bound)?)?>
            Iterator for const_source::$name<SR, CH, S$(,$t)?> {
            type Item = crate::Sample;

            fn next(&mut $self) -> Option<Self::Item> {
                $body
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.inner.size_hint()
            }
        }
    }
}

pub(crate) use inner;
pub(crate) use pure_effect;
