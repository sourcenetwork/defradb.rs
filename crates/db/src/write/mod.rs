//! The write path: the mutators that produce document deltas.
pub mod autocommit;
pub mod doc;
pub(crate) mod prepare;
pub mod queue;
