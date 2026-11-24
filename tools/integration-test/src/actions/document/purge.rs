use crate::client_type::ClientType;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.purge` op: `DbTransactionRegistry::purge_by_doc_ids_as`
/// in process, `DELETE /collections/{name}/documents/purge` over HTTP, and
/// `client collection purge-docs` through the CLI, as Go shinzo names them.
/// Go's parity pin has no document purge, so go-http does not run it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeDocs {
    pub collection: String,
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    /// How many documents the purge must report.
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
        let Some(node) = state.node(self.common.node())?.node.clone() else {
            return Err(Failure::unsupported(
                "purge on a node without its core: it has no HTTP or CLI route",
            ));
        };
        let doc_ids = self
            .doc_ids
            .iter()
            .map(|id| document::DocID::from_string(id.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Failure::operation(e.to_string()))?;
        let purged = state
            .block_on(node.txn_registry().purge_by_doc_ids_as(
                &state.context(self.txn),
                &self.collection,
                &doc_ids,
            ))
            .map_err(|e| Failure::operation(e.to_string()))?;
        self.check_count(purged)
    }
}

impl PurgeDocs {
    fn check_count(&self, purged: usize) -> Result<Value, Failure> {
        if let Some(expected) = self.expected_count {
            if purged != expected {
                return Err(Failure::operation(format!(
                    "expected {expected} purged documents; got {purged}"
                )));
            }
        }
        Ok(json!({}))
    }

    /// The count a wire response carries.
    fn wire_count(response: &Value) -> Result<usize, Failure> {
        response["count"]
            .as_u64()
            .map(|count| count as usize)
            .ok_or_else(|| {
                Failure::operation(format!("purge response without a count: {response}"))
            })
    }

    fn doc_id_strings(&self) -> Vec<String> {
        self.doc_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }
}

impl HttpRun for PurgeDocs {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            if state.client_type() == ClientType::GoHttp {
                return Err(Failure::unsupported(
                    "purge under GoHttp: Go's parity pin has no document purge",
                ));
            }
            action.common.plain(Self::name())?;
            let txn = state.txn_id(action.txn);
            let (collection, doc_ids) = (action.collection.clone(), action.doc_id_strings());
            let response = state
                .http_as(action.common.node(), None, |client| async move {
                    client
                        .collection_purge_docs(txn.as_deref(), &collection, &doc_ids)
                        .await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            action.check_count(Self::wire_count(&response)?)
        });
    }
}

impl CliRun for PurgeDocs {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let doc_ids = action.doc_id_strings();
            let mut args = vec![
                "collection",
                "--collection-name",
                &action.collection,
                "purge-docs",
            ];
            for id in &doc_ids {
                args.extend(["--docID", id.as_str()]);
            }
            let response = state
                .defra_client_as(action.common.node(), action.txn, None, &args)
                .map_err(Failure::operation)?;
            action.check_count(Self::wire_count(&response)?)
        });
    }
}

impl Action for PurgeDocs {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
