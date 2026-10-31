use crate::clients::{CliRun, HttpRun};
use crate::scenario::Text;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use query::documents::{AddOptions, Error as DocumentsError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Adds a document to a collection, or saves it over the one it has: one document (`doc_map`) or several (`docs`), encrypted or not.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDoc {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "document")]
    pub doc_map: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "documents")]
    pub docs: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "encryptDoc")]
    pub is_doc_encrypted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "docID")]
    pub doc_id: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_signing: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub upsert: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDoc {
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
        self.check_arguments()?;
        let (node, context, options) = (
            state.node_on(self.common.node())?,
            state.context(self.txn),
            self.options(),
        );
        let failed = |e: defra_node::Error| Failure::operation(e.to_string());
        if let Some(docs) = &self.docs {
            let ids = state
                .block_on(node.add_docs(&context, &self.collection, docs, &options))
                .map_err(failed)?;
            return Ok(json!({ "docIDs": ids }));
        }
        let doc = self.doc();
        let id = match self.upsert {
            true => state
                .block_on(node.save_doc(
                    &context,
                    &self.collection,
                    self.doc_id.as_ref().map(Text::as_str),
                    &doc,
                    &options,
                ))
                .map_err(failed)?,
            false => first(
                state
                    .block_on(node.add_docs(&context, &self.collection, &[doc], &options))
                    .map_err(failed)?,
            )?,
        };
        Ok(json!({ "docID": id }))
    }
}

impl AddDoc {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        if self.enable_signing.is_some() {
            return Err(Failure::unsupported(format!(
                "{} argument enableSigning",
                Self::name()
            )));
        }
        Ok(())
    }

    fn options(&self) -> AddOptions {
        AddOptions {
            encrypt_doc: self.is_doc_encrypted.unwrap_or_default(),
            encrypted_fields: self.encrypted_fields.clone().unwrap_or_default(),
        }
    }

    fn doc(&self) -> Value {
        self.doc_map.clone().unwrap_or_else(|| json!({}))
    }
}

/// The ID of the one document an add returned.
fn first(ids: Vec<String>) -> Result<String, Failure> {
    ids.into_iter()
        .next()
        .ok_or_else(|| Failure::operation(DocumentsError::DocumentNotFound.to_string()))
}

impl Action for AddDoc {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddDoc {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let (txn, options) = (state.txn_id(action.txn), action.options());
            let collection = action.collection.clone();
            let failed = |e: cli::error::Error| Failure::operation(e.to_string());
            if let Some(docs) = action.docs.clone() {
                let ids = state
                    .http_on(action.common.node(), |client| async move {
                        client
                            .add_docs(txn.as_deref(), &collection, &docs, &options)
                            .await
                    })?
                    .map_err(failed)?;
                return Ok(json!({ "docIDs": ids }));
            }
            let doc = action.doc();
            let id = match action.upsert {
                true => {
                    let doc_id = action.doc_id.as_ref().map(|id| id.as_str().to_owned());
                    state
                        .http_on(action.common.node(), |client| async move {
                            client
                                .save_doc(
                                    txn.as_deref(),
                                    &collection,
                                    doc_id.as_deref(),
                                    &doc,
                                    &options,
                                )
                                .await
                        })?
                        .map_err(failed)?
                }
                false => first(
                    state
                        .http_on(action.common.node(), |client| async move {
                            client
                                .add_docs(txn.as_deref(), &collection, &[doc], &options)
                                .await
                        })?
                        .map_err(failed)?,
                )?,
            };
            Ok(json!({ "docID": id }))
        });
    }
}

impl CliRun for AddDoc {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            if action.upsert {
                return Err(Failure::unsupported(format!(
                    "{} upsert: the CLI has no save command",
                    Self::name()
                )));
            }
            let (single, docs) = match &action.docs {
                Some(docs) => (false, Value::Array(docs.clone())),
                None => (true, action.doc()),
            };
            let docs = docs.to_string();
            let fields = action.options().encrypted_fields.join(",");
            let mut args = vec![
                "document",
                "--collection-name",
                &action.collection,
                "add",
                &docs,
            ];
            if action.options().encrypt_doc {
                args.push("--encrypt");
            }
            if !fields.is_empty() {
                args.extend(["--encrypt-fields", &fields]);
            }
            let printed = state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map_err(Failure::operation)?;
            let ids: Vec<String> = serde_json::from_value(printed)
                .map_err(|e| Failure::operation(format!("defra client printed no IDs: {e}")))?;
            match single {
                true => Ok(json!({ "docID": first(ids)? })),
                false => Ok(json!({ "docIDs": ids })),
            }
        });
    }
}
