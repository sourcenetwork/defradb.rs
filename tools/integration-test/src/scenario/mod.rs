//! The engine the scenario actions run on: the shared options every action
//! carries, the expectation matchers, and how an action's result is checked.

mod common;
mod error;
pub mod identity;
pub mod m;
pub mod matcher;
pub mod normalize;
pub mod number;
mod op;
pub mod pointer;
mod run;
mod text;

pub use common::{captures, Common, Eventually, DEFAULT_NODE};
pub use error::ScenarioError;
pub use matcher::{match_expected, validate, Bindings, Options};
pub use op::{Failure, FailureKind, Op};
pub use run::{execute, execute_with};
pub use text::Text;
