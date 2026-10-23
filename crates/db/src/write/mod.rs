//! The write path: the mutators that produce document deltas.
pub mod autocommit;
pub(crate) mod create;
pub mod doc;
pub(crate) mod prepare;
