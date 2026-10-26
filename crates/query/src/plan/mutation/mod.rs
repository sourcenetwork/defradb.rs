//! Mutation nodes for write operations

mod add;
mod create_conversions;
mod delete;
mod update;
mod upsert;

pub use add::{AddInput, AddNode};
pub use create_conversions::{
    json_to_normal_value, json_to_normal_value_with_kind, normal_value_to_json,
};
pub use delete::DeleteNode;
pub use update::{UpdateInput, UpdateNode};
pub use upsert::{UpsertAction, UpsertInput, UpsertNode};
