//! `SIMILARITY` on a parent whose embedding lives on its `@chunk` collection.
//!
//! Each parent is ranked by its best chunk. A routed query reads the chunk
//! index nearest first and maps chunks to parents; an unrouted one scores each
//! parent from the chunks its current text lists.

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
