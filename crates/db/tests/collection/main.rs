//! Collection CRUD, caching, naming, ACP, selection and deletion.
#[path = "../common/mod.rs"]
mod common;

mod acp;
mod activation;
mod add_collection;
mod cache;
mod delete;
mod index_actions;
mod map;
mod name;
mod part_ids;
mod retriever;
mod schema_publication;
mod schema_write_conflicts;
mod selector;
mod snapshot;
mod stream;
mod truncate;
mod validation;

mod series;
