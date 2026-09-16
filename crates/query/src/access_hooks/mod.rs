//! App-supplied, node-local gates on what clients of this node read and
//! write. They narrow ACP and never widen it.

pub(crate) mod read;
mod write;

pub use read::{AppReadCheck, ReadRequest, ReadValidator};
pub use write::{WriteRequest, WriteValidator};
