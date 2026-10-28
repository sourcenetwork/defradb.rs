mod collection;
mod expect;
mod query;

pub use collection::*;
pub use query::*;

use testo::derive::Action;
use testo::prelude::*;

use crate::test_case::State;

#[derive(Clone, Debug, Action)]
#[TState = "State"]
pub enum Actions {
    AddCollection(AddCollection),
    IntrospectionRequest(IntrospectionRequest),
}
