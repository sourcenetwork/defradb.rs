use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::common::Common;
use crate::test_case::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Operation,
    TransactionConflict,
    Unsupported,
}

/// An op that did not return a result.
#[derive(Clone, Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}

impl Failure {
    pub fn unsupported<S>(what: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            kind: FailureKind::Unsupported,
            message: what.into(),
        }
    }

    pub fn operation(message: String) -> Self {
        let kind = match message.contains("transaction conflict") {
            true => FailureKind::TransactionConflict,
            false => FailureKind::Operation,
        };
        Self { kind, message }
    }
}

/// One op of the scenario protocol: its own fields, its expectation and its
/// shared options, and how it runs. An op the client cannot do yet keeps the
/// default `run`, which fails with `unsupported op`.
pub trait Op:
    Clone + Debug + Default + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
    /// The action's name, for messages.
    fn name() -> &'static str {
        let full = std::any::type_name::<Self>();
        full.rsplit("::").next().unwrap_or(full)
    }

    /// The op answers with a GraphQL response whose errors are the action's
    /// errors, and `expected_data` is the response's `data`.
    const GRAPHQL: bool = false;

    fn common(&self) -> &Common;

    /// What the result must match; `null` when it is not checked.
    fn expected_data(&self) -> &Value;

    /// A substring of the error the action must fail with.
    fn expected_error(&self) -> Option<&str>;

    /// The response's `extensions` when the action checks them.
    fn expected_extensions(&self) -> &Value {
        &Value::Null
    }

    fn run(&self, _state: &State) -> Result<Value, Failure> {
        Err(Failure::unsupported(Self::name()))
    }

    /// Runs an op that changes the state itself (starts or stops a node);
    /// every other op only reads it.
    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.run(state)
    }
}
