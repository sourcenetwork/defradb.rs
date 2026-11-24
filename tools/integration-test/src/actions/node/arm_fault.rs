use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The failure `ArmCommitFault` makes every later log sync of the node fail with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitFault {
    /// `EIO`: the commit answers HTTP 500.
    #[default]
    Io,
    /// `ENOSPC`: the commit answers HTTP 507.
    StorageFull,
}

/// Makes every later commit of a node started with `commitFault` fail in its
/// write-ahead-log sync, until the node restarts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmCommitFault {
    #[serde(default)]
    pub fault: CommitFault,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ArmCommitFault {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let node = state.node(self.common.node())?;
        let env = node.spec.fault.as_ref().ok_or_else(|| {
            Failure::operation(format!(
                "node {} was not started with commitFault",
                self.common.node()
            ))
        })?;
        env.arm(match self.fault {
            CommitFault::Io => storage::testing::Fault::Io,
            CommitFault::StorageFull => storage::testing::Fault::StorageFull,
        });
        Ok(json!({}))
    }
}

impl Action for ArmCommitFault {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
