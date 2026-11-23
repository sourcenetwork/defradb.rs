//! Index endpoint handlers.
//!
//! Routes are Go's: /api/v0/collections/{name}/indexes and /api/v0/collections/indexes.
//!
//! All endpoints enforce NAC permissions when NAC is enabled.

mod go_api;

pub use go_api::{
    go_create_index, go_delete_index, go_list_all_indexes, go_list_indexes, GoCreateIndexRequest,
    GoIndexDescription, GoIndexedFieldDescription,
};
