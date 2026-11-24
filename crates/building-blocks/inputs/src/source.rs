//! The remote side of one input kind.

use std::fmt;
use std::hash::Hash;

use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSendSync};

/// What a round trip returns: one answer per key asked, in the order asked,
/// or an error that fails the whole round trip.
pub type Answers<V, E> = Result<Vec<Result<V, E>>, E>;

/// One input kind's remote side: how its keys group into round trips, one
/// round trip, which failures are worth another, and what may be cached.
///
/// ```
/// use defra_core::thread_bounds::MaybeBoxFuture;
///
/// struct Squares;
///
/// impl inputs::Source for Squares {
///     type Key = u64;
///     type Group = ();
///     type Value = u64;
///     type Error = String;
///
///     fn name(&self) -> &'static str {
///         "squares"
///     }
///     fn group(&self, _key: &u64) {}
///     fn fetch<'a>(
///         &'a self,
///         _group: &'a (),
///         keys: &'a [u64],
///     ) -> MaybeBoxFuture<'a, inputs::Answers<u64, String>> {
///         Box::pin(async move { Ok(keys.iter().map(|key| Ok(key * key)).collect()) })
///     }
///     fn retryable(&self, _error: &String) -> bool {
///         false
///     }
///     fn cache_bytes(&self, _key: &u64, _value: &u64) -> Option<usize> {
///         Some(16)
///     }
/// }
/// ```
pub trait Source: MaybeSendSync + 'static {
    /// One request: what single-flight and the cache are keyed by. Two keys
    /// are equal only when one answer serves both.
    type Key: Clone + Eq + Hash + MaybeSendSync + 'static;
    /// Keys of one group can share a round trip.
    type Group: Clone + Eq + Hash + MaybeSendSync + 'static;
    /// One key's answer.
    type Value: Clone + MaybeSendSync + 'static;
    /// The remote side's error.
    type Error: Clone + fmt::Display + MaybeSendSync + 'static;

    /// The input's name in metrics and logs.
    fn name(&self) -> &'static str;

    /// The group `key` rides in.
    fn group(&self, key: &Self::Key) -> Self::Group;

    /// One round trip for `keys`, all of `group`.
    fn fetch<'a>(
        &'a self,
        group: &'a Self::Group,
        keys: &'a [Self::Key],
    ) -> MaybeBoxFuture<'a, Answers<Self::Value, Self::Error>>;

    /// Whether a round trip that failed with `error` may be tried again.
    fn retryable(&self, error: &Self::Error) -> bool;

    /// The bytes `key`'s answer occupies in the cache, or `None` when it must
    /// not be cached.
    fn cache_bytes(&self, key: &Self::Key, value: &Self::Value) -> Option<usize>;
}
