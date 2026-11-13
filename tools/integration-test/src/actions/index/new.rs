use crate::actions::shared::IndexSpec;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use schema::IndexedFieldDescription;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
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
        let node = state.node_on(self.common.node())?;
        let context = state.context_as(None, self.common.identity_seed);
        // Go's NewIndex always returns before the build; its harness waits
        // for the build unless the scenario sets `async`.
        let created = match self.r#async {
            true => state.block_on(node.start_index_as(
                &context,
                &self.collection,
                self.index_name(),
                fields,
                self.unique(),
            )),
            false => state.block_on(node.create_index_as(
                &context,
                &self.collection,
                self.index_name(),
                fields,
                self.unique(),
            )),
        };
        created
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl NewIndex {
    /// The index's name; an empty one is generated.
    fn index_name(&self) -> Option<&str> {
        Some(self.index.name.as_str()).filter(|name| !name.is_empty())
    }

    /// Whether the index is unique, by its flag or its ordered config's.
    fn unique(&self) -> bool {
        let ordered = self
            .index
            .ordered
            .as_ref()
            .and_then(|config| config.get("Unique"));
        self.index.unique
            || ordered
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_default()
    }

    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.client_identity(Self::name_of())?;
        let ordered_unique = self
            .index
            .ordered
            .as_ref()
            .and_then(|config| config.get("Unique"))
            .and_then(serde_json::Value::as_bool);
        if ordered_unique.is_some_and(|unique| unique != self.index.unique) && self.index.unique {
            return Err(Failure::operation(
                "index request sets both the deprecated unique field and an ordered config that disagree"
                    .to_owned(),
            ));
        }
        if self.index.ordered.is_some() && self.index.vector.is_some() {
            return Err(Failure::operation(
                "index request has more than one kind config".to_owned(),
            ));
        }
        let unsupported = [
            ("txn", self.txn.is_some()),
            ("index.vector", self.index.vector.is_some()),
        ];
        unsupported_argument(&unsupported)
    }

    /// [`NewIndex::check_arguments`], plus the descending fields the HTTP and
    /// CLI clients cannot send, and `async`: an index created over HTTP is
    /// built before the response.
    fn check_remote_arguments(&self) -> Result<(), Failure> {
        self.check_arguments()?;
        let descending = self.index.fields.iter().any(|field| field.descending);
        unsupported_argument(&[
            ("index.fields.descending", descending),
            ("async", self.r#async),
        ])
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
            let unique = action.unique();
            let created = collection.clone();
            state
                .http_as(
                    action.common.node(),
                    action.common.identity_seed,
                    |client| async move {
                        client
                            .index_create(&created, &fields, name.as_deref(), unique, None)
                            .await
                    },
                )?
                .map_err(|e| Failure::operation(e.to_string()))?;
            wait_until_built(state, action, &collection)?;
            Ok(json!({}))
        });
    }
}

/// How long a backfill may take before the wait gives up.
const BUILD_TIMEOUT: Duration = Duration::from_secs(10);

/// Waits until no index of `collection` is still building, as Go's
/// `NewIndex` action does: Go builds an index in the background, so a
/// `ListIndexes` straight after the create would see it mid-backfill.
// ponytail: waits on every index of the collection, not just the new one; the
// HTTP client refuses `async`, so no other index is left building on purpose.
fn wait_until_built(state: &State, action: &NewIndex, collection: &str) -> Result<(), Failure> {
    let deadline = Instant::now() + BUILD_TIMEOUT;
    loop {
        let collection = collection.to_owned();
        let remaining = deadline.saturating_duration_since(Instant::now());
        let indexes = state
            .http_as(
                action.common.node(),
                action.common.identity_seed,
                |client| async move {
                    workers::timeout(remaining, client.index_listing(&collection))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())
                },
            )?
            .map_err(Failure::operation)?;
        if !indexes
            .as_array()
            .is_some_and(|all| all.iter().any(building))
        {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(Failure::operation(format!(
                "{}: an index of {} was still building after {BUILD_TIMEOUT:?}",
                NewIndex::name_of(),
                action.collection
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Go's building state: `BackfillIndexAction` (3) and `InProgressActionStatus` (1).
pub(crate) fn building(index: &Value) -> bool {
    let execution = &index["Execution"];
    execution["Action"] == 3 && execution["Status"] == 1
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
            if action.unique() {
                args.push("--unique");
            }
            state
                .defra_client_as(
                    action.common.node(),
                    None,
                    action.common.identity_seed,
                    &args,
                )
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
