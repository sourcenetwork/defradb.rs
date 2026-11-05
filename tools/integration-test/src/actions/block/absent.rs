use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Fails unless a Rust node's block store does not hold a block, reading the
/// store itself, under every client: a query cannot tell a block never stored
/// from one stored and owned by no document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockAbsent {
    #[serde(rename = "blockCID")]
    pub cid: Text,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for BlockAbsent {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &Value::Null
    }

    fn expected_error(&self) -> Option<&str> {
        None
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        let cid: cid::Cid = self
            .cid
            .as_str()
            .parse()
            .map_err(|error| Failure::operation(format!("invalid CID: {error}")))?;
        let node = state
            .node(self.common.node())?
            .core()
            .ok_or_else(|| Failure::unsupported("BlockAbsent reads a Rust node's store"))?;
        let held = state.block_on(async {
            let txn = node
                .database()
                .new_txn(true)
                .await
                .map_err(|error| error.to_string())?;
            let blockstore = txn.blockstore().map_err(|error| error.to_string())?;
            blockstore
                .get(&cid.to_bytes())
                .await
                .map_err(|error| error.to_string())
        });
        match held.map_err(Failure::operation)? {
            Some(_) => Err(Failure::operation(format!("block {cid} is stored"))),
            None => Ok(json!({})),
        }
    }
}

impl Action for BlockAbsent {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
