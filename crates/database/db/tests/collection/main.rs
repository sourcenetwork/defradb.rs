//! Collection CRUD, caching, naming, ACP, selection and deletion.
#[path = "../common/mod.rs"]
mod common;

mod acp;
mod add_collection;
mod cache;
mod create_gate;
mod delete;
mod encrypted_index_concurrent;
mod map;
mod name;
mod part_ids;
mod purge;
mod relation_identity;
mod retriever;
mod schema_publication;
mod schema_write_conflicts;
mod selector;
mod snapshot;
mod stream;
mod truncate;
mod validation;

mod series;
