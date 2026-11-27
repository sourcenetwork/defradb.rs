//! Transaction-scoped update events published to subscribers on commit.

pub(crate) mod arrivals;
pub mod emission;
mod gate;
pub(crate) mod updates;
