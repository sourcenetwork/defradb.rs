//! Collections: added from SDL, described, patched, activated, truncated
//! and deleted.

mod add;
mod delete;
mod describe;
mod patch;
pub(crate) mod select;
mod set_active;
mod truncate;

pub use add::defra_collection_add;
pub use delete::defra_collection_delete;
pub use describe::defra_collection_describe;
pub use patch::defra_collection_patch;
pub use set_active::defra_collection_set_active;
pub use truncate::{defra_collection_truncate, defra_collection_truncate_with_filter};

#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod tests;
