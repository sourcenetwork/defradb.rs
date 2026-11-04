use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scenario::Eventually;

/// Ops that the corpus spells twice: once plain and once `WithRetry`. A step
/// carries the plain name and a `retry` flag.
pub(super) const RETRY_TWINS: &[(&str, &str)] = &[
    ("document.createWithRetry", "document.create"),
    ("document.updateWithRetry", "document.update"),
    ("document.deleteWithRetry", "document.delete"),
    (
        "document.createEncryptedWithRetry",
        "document.createEncrypted",
    ),
    ("document.saveWithRetry", "document.save"),
    ("query.mutateWithRetry", "query.execute"),
];

/// What must hold after a step ran.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    pub expect: Option<Value>,
    pub expect_error: Option<String>,
    pub capture: BTreeMap<String, String>,
    pub unordered: Vec<String>,
    pub unordered_documents: Vec<String>,
    pub compare: Option<bool>,
    pub eventually: Option<Eventually>,
}

/// One scenario step as the corpus spells it: an op with its arguments, and what must hold after it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "Wire", into = "Wire")]
pub struct WireStep {
    pub op: String,
    pub args: Value,
    pub check: Check,
    pub retry: bool,
    /// A child of a parallel group: `expect` is matched against
    /// `{"result": .., "error": ..}` and a failed op does not fail the step.
    #[serde(skip)]
    pub envelope: bool,
}

impl WireStep {
    /// The op's name in the scenario corpus.
    pub fn wire_op(&self) -> String {
        if self.retry {
            if let Some((twin, _)) = RETRY_TWINS.iter().find(|(_, plain)| *plain == self.op) {
                return (*twin).to_owned();
            }
        }
        self.op.clone()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    op: String,
    #[serde(default)]
    args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expect: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expect_error: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    capture: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compare: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unordered: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unordered_documents: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    eventually: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    eventually_timeout_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    eventually_interval_millis: Option<u64>,
}

impl From<Wire> for WireStep {
    fn from(wire: Wire) -> Self {
        let twin = RETRY_TWINS.iter().find(|(name, _)| *name == wire.op);
        let (op, retry) = match twin {
            Some((_, plain)) => ((*plain).to_owned(), true),
            None => (wire.op, false),
        };
        let eventually = wire.eventually.then_some(Eventually {
            timeout_millis: wire.eventually_timeout_millis,
            interval_millis: wire.eventually_interval_millis,
        });
        WireStep {
            op,
            args: wire.args,
            check: Check {
                expect: wire.expect,
                expect_error: wire.expect_error,
                capture: wire.capture,
                unordered: wire.unordered,
                unordered_documents: wire.unordered_documents,
                compare: wire.compare,
                eventually,
            },
            retry,
            envelope: false,
        }
    }
}

impl From<WireStep> for Wire {
    fn from(step: WireStep) -> Self {
        let op = step.wire_op();
        let check = step.check;
        Wire {
            op,
            args: step.args,
            expect: check.expect,
            expect_error: check.expect_error,
            capture: check.capture,
            compare: check.compare,
            unordered: check.unordered,
            unordered_documents: check.unordered_documents,
            eventually: check.eventually.is_some(),
            eventually_timeout_millis: check.eventually.as_ref().and_then(|e| e.timeout_millis),
            eventually_interval_millis: check.eventually.as_ref().and_then(|e| e.interval_millis),
        }
    }
}
