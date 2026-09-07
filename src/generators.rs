//! Sources that produce sound without consuming anything

// Note we make everything be it effects or generators available under <source_type>/<thing>

mod silence;

/// Generators with both the sample rate and channel count known at compile time.
pub mod const_source {
    pub use super::silence::const_source::Silence;
}
