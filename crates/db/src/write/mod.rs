//! The write path: the mutators that produce document deltas.
pub(crate) mod counter;
pub(crate) mod create;
pub mod mutator;
pub(crate) mod persist;
pub mod queue;
