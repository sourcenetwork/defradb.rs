//! Secondary indexes: created, listed and deleted.

mod create;
mod manage;

pub use create::defra_index_new;
pub use manage::{defra_index_delete, defra_index_list};
