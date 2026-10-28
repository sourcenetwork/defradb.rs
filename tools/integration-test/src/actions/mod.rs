mod add_collection;
mod expect;
mod introspection_request;

pub use add_collection::AddCollection;
pub use introspection_request::IntrospectionRequest;

use testo::derive::Action;
use testo::prelude::*;

use crate::test_case::State;

#[derive(Clone, Debug, Action)]
#[TState = "State"]
pub enum Actions {
    AddCollection(AddCollection),
    IntrospectionRequest(IntrospectionRequest),
}
