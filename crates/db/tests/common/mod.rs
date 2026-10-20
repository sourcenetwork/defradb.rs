//! Fixtures shared by the db test binaries.
//!
//! Each binary pulls the whole module in and uses a subset, so unused items
//! here are expected rather than dead.
#![allow(dead_code)]

pub mod counting_store;
pub mod failing_write_store;
pub mod fixture;
pub mod guard_events;
pub mod http;
pub mod owned;
pub mod refusing_lens_store;
pub mod schema;
pub mod stream;
pub mod worker_io;
