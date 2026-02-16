use crate::source::{chain, Chain};

/// Builds a source that chains sources built from a factory function.
///
/// The `factory` parameter is a function that produces a source. The source is then played.
/// Whenever the source ends, `factory` is called again in order to produce the source that is
/// played next.
///
/// If the `factory` closure returns `None`, then the sound ends.
pub fn from_fn<F, S>(factory: F) -> Chain<FromFn<F>>
where
    F: FnMut() -> Option<S>,
{
    chain(FromFn { factory })
}

/// Deprecated: Use `from_fn()` instead.
#[deprecated(since = "0.22.0", note = "Use `from_fn()` instead")]
pub fn from_factory<F, S>(factory: F) -> Chain<FromFn<F>>
where
    F: FnMut() -> Option<S>,
{
    from_fn(factory)
}

/// Iterator that generates sources from a factory function.
///
/// Created by the `from_fn()` function.
pub struct FromFn<F> {
    factory: F,
}

/// Deprecated: Use `FromFn` instead.
#[deprecated(since = "0.22.0", note = "Use `FromFn` instead")]
pub type FromFactoryIter<F> = FromFn<F>;

impl<F, S> Iterator for FromFn<F>
where
    F: FnMut() -> Option<S>,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        (self.factory)()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}
