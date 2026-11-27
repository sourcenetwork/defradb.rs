//! Transaction-scoped update events published to subscribers on commit.

pub(crate) mod arrivals;
pub mod emission;
mod gate;
pub(crate) mod update_retention;
pub(crate) mod updates;
