//! Views: adding, refreshing and collecting them, and the materialized-view
//! refresh itself.

mod client;
pub(crate) mod ops;
mod selection;

pub use client::ViewError;
