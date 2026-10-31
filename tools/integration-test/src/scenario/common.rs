use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::op::Failure;

/// The node a step runs on when it names none.
pub const DEFAULT_NODE: &str = "db";

/// What every action can say besides its own fields: where and as whom it
/// runs, how it is retried or polled, what it captures, and how its result is
/// compared. Every action embeds it, so a test writes
/// `common: Common { retry: true, ..Default::default() }` only when it needs
/// one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Common {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_node: Option<String>,
    /// Run the action again when it loses a transaction conflict.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retry: bool,
    /// The action is a child of a parallel group: its expectation is matched
    /// against `{"result": .., "error": ..}` and a failed action does not fail
    /// the test.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub envelope: bool,
    /// Names for parts of the result, by JSON pointer, for later actions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capture: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<bool>,
    /// JSON pointers of arrays the expectation matches in any order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unordered: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unordered_documents: Vec<String>,
    /// Poll the action until its expectation holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eventually: Option<Eventually>,
}

/// Captures from `(name, JSON pointer)` pairs.
pub fn captures<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(name, pointer)| (name.to_owned(), pointer.to_owned()))
        .collect()
}

impl Common {
    pub fn node(&self) -> &str {
        self.node.as_deref().unwrap_or(DEFAULT_NODE)
    }

    /// Fails with `unsupported` unless the action runs as the ambient
    /// identity, which is all the clients offer yet.
    pub fn plain(&self, op: &str) -> Result<(), Failure> {
        if self.identity_seed.is_some() {
            return Err(Failure::unsupported(format!("{op} argument identitySeed")));
        }
        match self.identity_node.is_some() {
            true => Err(Failure::unsupported(format!("{op} argument identityNode"))),
            false => Ok(()),
        }
    }
}

/// How an action is polled until its expectation holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eventually {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_millis: Option<u64>,
}
