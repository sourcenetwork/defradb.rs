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
    /// The commit outcome the client was told (`aborted`, `indeterminate`),
    /// when the failure carried one.
    pub outcome: Option<String>,
    /// The CLI's exit status, when the failure came from the CLI.
    pub exit_code: Option<i32>,
}

impl Failure {
    pub fn unsupported<S>(what: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            kind: FailureKind::Unsupported,
            message: what.into(),
            outcome: None,
            exit_code: None,
        }
    }

    pub fn operation(message: String) -> Self {
        let kind = match message.contains("transaction conflict") {
            true => FailureKind::TransactionConflict,
            false => FailureKind::Operation,
        };
        Self {
            kind,
            message,
            outcome: None,
            exit_code: None,
        }
    }

    /// The failure with the commit outcome its client reported.
    pub fn with_outcome(mut self, outcome: Option<defra_core::CommitOutcome>) -> Self {
        self.outcome = outcome.and_then(|outcome| outcome.failure_str().map(str::to_owned));
        self
    }

    /// A failed CLI run: its stderr, and the outcome its exit status names.
    pub fn cli(failure: crate::clients::CliFailure) -> Self {
        let outcome = match failure.status {
            Some(3) => Some("aborted".to_owned()),
            Some(4) => Some("indeterminate".to_owned()),
            _ => None,
        };
        Self {
            outcome,
            exit_code: failure.status,
            ..Self::operation(failure.stderr)
        }
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

    /// The `extensions.code` the GraphQL error matching `expected_error` must
    /// carry.
    fn expected_error_code(&self) -> Option<&str> {
        None
    }

    /// The response's `extensions` when the action checks them.
    fn expected_extensions(&self) -> &Value {
        &Value::Null
    }

    /// The op once its captures are resolved, before it runs.
    fn resolved(self) -> Self {
        self
    }

    /// The commit outcome the failure must carry (`aborted`, `indeterminate`).
    fn expected_commit_outcome(&self) -> Option<&str> {
        None
    }

    /// The CLI exit status the failure must have; checked only under the CLI.
    fn expected_exit_code(&self) -> Option<i32> {
        None
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
