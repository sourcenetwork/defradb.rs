use crate::actions::peer::support::{addresses_of, failed, REPLICATOR_COMPLETED};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use defra_node::{ReplicationFilter, ReplicationFilters};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `replicator.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReplicator {
    pub target: String,
    /// The collections to push; none pushes every collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    /// Query-filter conditions every listed collection's documents must
    /// match to be pushed, as the CLI's `--filter` takes them.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub filter: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl AddReplicator {
    fn filters(&self) -> ReplicationFilters {
        let Some(conditions) = self.filter.as_object() else {
            return ReplicationFilters::new();
        };
        self.collections
            .iter()
            .map(|collection| {
                (
                    collection.clone(),
                    ReplicationFilter::predicate(conditions.clone()),
                )
            })
            .collect()
    }
}

impl Op for AddReplicator {
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
        let identity = self.common.client_identity(Self::name())?;
        let addresses = addresses_of(state, &[self.target.as_str()])?;
        let (test_node, node) = (
            state.node(self.common.node())?,
            state.node_on(self.common.node())?,
        );
        test_node
            .replicator_completed(
                || {
                    state.block_on(node.add_replicator_as(
                        &state.context_as(None, identity),
                        &addresses,
                        &self.collections,
                        self.filters(),
                    ))
                },
                REPLICATOR_COMPLETED,
            )
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl Action for AddReplicator {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddReplicator {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let addresses = addresses_of(state, &[action.target.as_str()])?;
            let (collections, filters) = (action.collections.clone(), action.filters());
            let test_node = state.node(action.common.node())?;
            test_node
                .replicator_completed(
                    || {
                        state
                            .http_as(action.common.node(), identity, |client| async move {
                                match filters.is_empty() {
                                    true => client.add_replicator(&addresses, &collections).await,
                                    false => {
                                        client
                                            .p2p_replicator_add(
                                                &collections,
                                                addresses.first().map(String::as_str),
                                                filters,
                                                &[],
                                            )
                                            .await
                                    }
                                }
                            })?
                            .map_err(failed)
                    },
                    REPLICATOR_COMPLETED,
                )
                .map(|()| json!({}))
        });
    }
}

impl CliRun for AddReplicator {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let addresses = addresses_of(state, &[action.target.as_str()])?;
            let (collections, filter) = (action.collections.join(","), action.filter.to_string());
            let mut args = vec!["p2p", "replicator", "add"];
            if !action.collections.is_empty() {
                args.extend(["-c", collections.as_str()]);
            }
            if !action.filter.is_null() {
                args.extend(["--filter", filter.as_str()]);
            }
            args.extend(addresses.iter().map(String::as_str));
            state
                .node(action.common.node())?
                .replicator_completed(
                    || {
                        state
                            .defra_client_as(action.common.node(), None, identity, &args)
                            .map_err(Failure::operation)
                    },
                    REPLICATOR_COMPLETED,
                )
                .map(|_| json!({}))
        });
    }
}
