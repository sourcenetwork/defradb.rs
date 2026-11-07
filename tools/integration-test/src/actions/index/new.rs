use crate::actions::shared::IndexSpec;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use schema::IndexedFieldDescription;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Creates an index on a collection.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIndex {
    pub collection: String,
    pub index: IndexSpec,
    #[serde(default, rename = "async", skip_serializing_if = "std::ops::Not::not")]
    pub r#async: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for NewIndex {
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
        let fields = self
            .index
            .fields
            .iter()
            .map(|field| IndexedFieldDescription {
                name: field.name.clone(),
                descending: field.descending,
            })
            .collect();
        state
            .block_on(state.node_on(self.common.node())?.create_index_as(
                &state.context(None),
                &self.collection,
                self.index_name(),
                fields,
                self.index.unique,
            ))
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl NewIndex {
    /// The index's name; an empty one is generated.
    fn index_name(&self) -> Option<&str> {
        Some(self.index.name.as_str()).filter(|name| !name.is_empty())
    }

    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name_of())?;
        let unsupported = [
            ("async", self.r#async),
            ("txn", self.txn.is_some()),
            ("index.ordered", self.index.ordered.is_some()),
            ("index.vector", self.index.vector.is_some()),
        ];
        unsupported_argument(&unsupported)
    }

    /// [`NewIndex::check_arguments`], plus the descending fields the HTTP and
    /// CLI clients cannot send.
    fn check_remote_arguments(&self) -> Result<(), Failure> {
        self.check_arguments()?;
        let descending = self.index.fields.iter().any(|field| field.descending);
        unsupported_argument(&[("index.fields.descending", descending)])
    }

    fn name_of() -> &'static str {
        <Self as Op>::name()
    }

    fn field_names(&self) -> Vec<String> {
        self.index.fields.iter().map(|f| f.name.clone()).collect()
    }
}

fn unsupported_argument(arguments: &[(&str, bool)]) -> Result<(), Failure> {
    match arguments.iter().find(|(_, set)| *set) {
        Some((name, _)) => Err(Failure::unsupported(format!(
            "{} argument {name}",
            NewIndex::name_of()
        ))),
        None => Ok(()),
    }
}

impl Action for NewIndex {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for NewIndex {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_remote_arguments()?;
            let collection = action.collection.clone();
            let fields = action.field_names();
            let name = action.index_name().map(str::to_owned);
            let unique = action.index.unique;
            state
                .http_on(action.common.node(), |client| async move {
                    client
                        .index_create(&collection, &fields, name.as_deref(), unique, None)
                        .await
                })?
                .map(|_| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for NewIndex {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_remote_arguments()?;
            let fields = action.field_names().join(",");
            let mut args = vec![
                "index",
                "new",
                "--collection",
                action.collection.as_str(),
                "--fields",
                fields.as_str(),
            ];
            if let Some(name) = action.index_name() {
                args.extend(["--name", name]);
            }
            if action.index.unique {
                args.push("--unique");
            }
            state
                .defra_client_on(action.common.node(), None, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
