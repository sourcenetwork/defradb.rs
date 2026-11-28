use crate::actions::shared::IndexSpec;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use schema::{IndexedFieldDescription, VectorIndexDescription};
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
        let scope = state.scope_as(self.txn, self.common.identity_seed);
        let vector = self.vector_description()?;
        // Go's NewIndex always returns before the build; its harness waits
        // for the build unless the scenario sets `async`.
        let created = match self.r#async {
            true => state.block_on(scope.on(&node).start_index(
                &self.collection,
                self.index_name(),
                fields,
                self.unique(),
                vector,
            )),
            false => state.block_on(scope.on(&node).create_index(
                &self.collection,
                self.index_name(),
                fields,
                self.unique(),
                vector,
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

    fn vector_description(&self) -> Result<Option<VectorIndexDescription>, Failure> {
        let Some(vector) = self.index.vector.as_ref() else {
            return Ok(None);
        };
        serde_json::from_value(vector.clone())
            .map(Some)
            .map_err(|error| Failure::operation(error.to_string()))
    }

    /// Fails arguments every client refuses before sending a request.
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
        if self
            .index
            .vector
            .as_ref()
            .is_some_and(|vector| !vector.is_object())
        {
            return Err(Failure::operation(
                "index.vector must be a JSON object".to_owned(),
            ));
        }
        Ok(())
    }

    /// [`NewIndex::check_arguments`], plus descending fields, which neither
    /// remote client can send.
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
            let unique = action.unique();
            let target = collection.clone();
            let txn = state.txn_id(action.txn);
            let in_txn = txn.is_some();
            if action.r#async && state.client_type() == crate::ClientType::GoHttp {
                return Err(Failure::unsupported(
                    "index.new async under GoHttp".to_owned(),
                ));
            }
            let wait_for_build = !action.r#async && !in_txn;
            let build_in_background = action.r#async && !in_txn;
            let vector = action.index.vector.clone();
            let created = state
                .http_as(
                    action.common.node(),
                    action.common.identity_seed,
                    |client| async move {
                        client
                            .index_create_vector_with_build_mode(
                                txn.as_deref(),
                                &target,
                                &fields,
                                name.as_deref(),
                                unique,
                                vector,
                                build_in_background,
                            )
                            .await
                    },
                )?
                .map_err(|e| Failure::operation(e.to_string()))?;
            // In a transaction the backfill runs once it commits; an async
            // build is left to the scenario's own wait actions.
            if wait_for_build {
                wait_until_built(state, action, &collection, &created.name)?;
            }
            Ok(json!({}))
        });
    }
}

/// How long a backfill may take before the wait gives up.
const BUILD_TIMEOUT: Duration = Duration::from_secs(10);

/// Waits until the index `name` of `collection` is no longer building, as
/// Go's `NewIndex` action does: Go builds an index in the background, so a
/// `ListIndexes` straight after the create would see it mid-backfill. Another
/// index of the collection still building in the background does not hold it.
fn wait_until_built(
    state: &State,
    action: &NewIndex,
    collection: &str,
    name: &str,
) -> Result<(), Failure> {
    let deadline = Instant::now() + BUILD_TIMEOUT;
    loop {
        let collection = collection.to_owned();
        let remaining = deadline.saturating_duration_since(Instant::now());
        let indexes = state
            .http_as(
                action.common.node(),
                action.common.identity_seed,
                |client| async move {
                    workers::timeout(remaining, client.index_listing(None, &collection))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())
                },
            )?
            .map_err(Failure::operation)?;
        let still_building = indexes.as_array().is_some_and(|all| {
            all.iter()
                .any(|index| index["Description"]["Name"] == name && building(index))
        });
        if !still_building {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(Failure::operation(format!(
                "{}: index {name} of {} was still building after {BUILD_TIMEOUT:?}",
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
            // `index new` has no background flag, here or in Go's CLI.
            unsupported_argument(&[("async", action.r#async)])?;
            let fields = action.field_names().join(",");
            // Held here so the borrowed flag below outlives the argument list.
            let as_json = action
                .index
                .vector
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| Failure::operation(error.to_string()))?;
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
            if let Some(as_json) = &as_json {
                args.extend(["--vector", as_json.as_str()]);
            }
            state
                .defra_client_as(
                    action.common.node(),
                    action.txn,
                    action.common.identity_seed,
                    &args,
                )
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}
