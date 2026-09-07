//! Code shared between source types.
//!
//! Since we have three source types there is a lot of code duplication. We
//! combat that by placing whatever can be shared here.
//!
//! This can be:
//! - shared types like error enums
//! - shared free functions
//! - shared members / impl blocks through macro_rules
//!
//! # Note
//! Effects are defined through a macro and do not need this kind of
//! deduplication
//!
//! This modules structure mirrors that of what it deduplicates. For example
//! the code shared between [fixed_source::chain] and [const_source::chain] is in
//! common/source/chain.rs

pub(crate) mod buffer;
pub(crate) mod chain;
