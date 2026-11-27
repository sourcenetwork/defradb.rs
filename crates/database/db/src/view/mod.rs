//! Views: adding, refreshing and collecting them, and the materialized-view
//! refresh itself.

mod client;
pub(crate) mod ops;

pub use client::ViewError;
