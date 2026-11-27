//! `SIMILARITY` on a parent whose embedding lives on its `@chunk` collection.

mod best;
mod listed;
mod node;
mod routed;
mod search;
mod target;

pub use node::ChunkSimilarityNode;
pub(crate) use node::ListedInRow;
pub use routed::ChunkSimilarityScanNode;
pub(crate) use target::ChunkTarget;
