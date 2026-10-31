use crate::client_type::network_edge;
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.purge` op: Go Shinzo's `PurgeByDocIDs`. A Rust node purges
/// through its core; a Go Shinzo node through
/// `POST /collections/{name}/documents/purge`, which reports no count.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeDocs {
    pub collection: String,
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<Text>,
    #[serde(default)]
    pub prune_history: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    /// How many documents the purge must report, where the node reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PurgeDocs {
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
        let doc_ids: Vec<String> = self
            .doc_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        let Some(node) = node.node.clone() else {
            return self.run_go(state, doc_ids);
        };
        let doc_ids = doc_ids
            .iter()
            .map(|id| document::DocID::from_string(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Failure::operation(e.to_string()))?;
        let purged = state
            .block_on(node.txn_registry().purge_by_doc_ids_as(
                &state.context(self.txn),
                &self.collection,
                &doc_ids,
                self.prune_history,
            ))
            .map_err(|e| Failure::operation(e.to_string()))?;
        if let Some(expected) = self.expected_count {
            if purged != expected {
                return Err(Failure::operation(format!(
                    "expected {expected} purged documents; got {purged}"
                )));
            }
        }
        Ok(json!({}))
    }
}

impl PurgeDocs {
    fn run_go(&self, state: &State, doc_ids: Vec<String>) -> Result<Value, Failure> {
        let url = format!(
            "{}/api/v0/collections/{}/documents/purge",
            state.url_on(self.common.node())?,
            self.collection
        );
        let body = json!({ "docIDs": doc_ids, "pruneHistory": self.prune_history });
        let txn = state.txn_id(self.txn);
        let sent = state
            .block_on(network_edge().spawn(async move {
                let mut request = reqwest::Client::new().post(url).json(&body);
                if let Some(txn) = txn {
                    request = request.header("x-defradb-tx", txn);
                }
                let response = request.send().await.map_err(|e| e.to_string())?;
                let status = response.status();
                let text = response.text().await.map_err(|e| e.to_string())?;
                Ok::<_, String>((status, text))
            }))
            .expect("run on the network edge");
        let (status, text) = sent.map_err(Failure::operation)?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|body| body.get("error").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or(text);
            return Err(Failure::operation(message));
        }
        Ok(json!({}))
    }
}

impl Action for PurgeDocs {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
