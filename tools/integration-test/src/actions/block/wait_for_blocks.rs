use std::sync::Arc;
use std::time::{Duration, Instant};

use blockstore::Blockstore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;

/// How long the blocks may take to arrive.
const ARRIVAL_TIMEOUT: Duration = Duration::from_secs(20);
const ARRIVAL_POLL: Duration = Duration::from_millis(100);

/// Waits until the node's P2P blockstore holds every block in `cids`, whether
/// or not they merged. A peer without a document's key stores its blocks but
/// never merges them, so no merge event shows they arrived.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForBlocks {
    pub cids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForBlocks {
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
        let core = state.node(self.common.node())?.core().ok_or_else(|| {
            Failure::unsupported(format!("{} on a node in another process", Self::name()))
        })?;
        let cids = self
            .cids
            .iter()
            .map(|cid| {
                cid::Cid::try_from(cid.as_str())
                    .map_err(|e| Failure::operation(format!("{}: {e}", Self::name())))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let blocks = blockstore::DefraBlockstore::new(Arc::clone(core.database().store()), true);
        let deadline = Instant::now() + ARRIVAL_TIMEOUT;
        loop {
            let present = state
                .block_on(blocks.has_many(&cids))
                .map_err(|e| Failure::operation(e.to_string()))?;
            if present.iter().all(|&held| held) {
                return Ok(json!({}));
            }
            if Instant::now() > deadline {
                return Err(Failure::operation(format!(
                    "{}: the node did not receive every block within {ARRIVAL_TIMEOUT:?}",
                    Self::name()
                )));
            }
            std::thread::sleep(ARRIVAL_POLL);
        }
    }
}

impl Action for WaitForBlocks {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
