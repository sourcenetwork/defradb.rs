//! Indexes over blind fields, kept up to date after the write.
//!
//! A blind write (a counter increment) never learns the value it lands on, so
//! it cannot move its index entry. It appends the document to the index's
//! pending log instead, which never conflicts. A drain later moves the entry to
//! the live value, finding the old one through the document's position record,
//! and truncates the log through what it planned. Until then a read through the
//! index checks the documents still in the log live (`proofs/tla/DeferredIndex.tla`).

mod cleanup;
mod drain;
mod overlay;
mod record;
mod requests;
mod task;

pub(crate) use cleanup::remove_page;
pub(crate) use record::{decode_record, encode_record};
pub use requests::DrainRequests;
pub(crate) use requests::IndexRef;
