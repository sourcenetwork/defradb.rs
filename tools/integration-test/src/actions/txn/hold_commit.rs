use std::time::Duration;

use crate::client_type::{network_edge, ClientType};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Commits a transaction past its HTTP bound, while its commit stays held.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldCommit {
    pub txn: usize,
    pub expected_error: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for HoldCommit {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        Some(&self.expected_error)
    }

    /// The commit's answer as an error, for the scenario to match: the
    /// bound's 503 with `outcome: "unknown"`, given while the commit is held
    /// at the gate.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        let id = state
            .txns
            .get(self.txn)
            .ok_or_else(|| Failure::operation(format!("no transaction {}", self.txn)))?
            .to_string();
        let node = state.node(self.common.node())?;
        let gate = node
            .spec
            .commit_gate
            .clone()
            .ok_or_else(|| Failure::operation("the node has no commit gate".to_owned()))?;
        let error = match state.client_type() {
            ClientType::RustHttp => {
                let url = state.url_on(self.common.node())?;
                let (status, body) = state
                    .block_on(network_edge().spawn(async move {
                        let client = net::http::Client::new(network_edge());
                        let mut request = client.post(format!("{url}/api/v0/tx/{id}"));
                        if let Some(seed) = identity {
                            let token = cli::commands::client::generate_auth_token(
                                &crate::scenario::identity::private_key_hex(seed),
                                &url,
                            )
                            .expect("a token for a test identity");
                            request = request.header("Authorization", format!("Bearer {token}"));
                        }
                        let response = request.body("").send().await?;
                        let status = response.status().as_u16();
                        Ok::<_, net::http::Error>((status, response.text().await?))
                    }))
                    .map_err(|error| Failure::operation(error.to_string()))?
                    .map_err(|error| Failure::operation(error.to_string()))?;
                let value: Value = serde_json::from_str(&body).map_err(|error| {
                    Failure::operation(format!("bad bounded-commit body {body:?}: {error}"))
                })?;
                if status != 503 || value["outcome"] != json!("unknown") {
                    return Err(Failure::operation(format!(
                        "the bounded commit answered {status}: {value}"
                    )));
                }
                value["error"].as_str().unwrap_or_default().to_owned()
            }
            ClientType::Cli => match state.defra_client_as(
                self.common.node(),
                None,
                identity,
                &["tx", "commit", &id],
            ) {
                Ok(value) => {
                    return Err(Failure::operation(format!(
                        "the bounded commit unexpectedly succeeded: {value}"
                    )))
                }
                Err(error) => error,
            },
            client => {
                return Err(Failure::unsupported(format!(
                    "{} under {client:?}",
                    Self::name()
                )))
            }
        };
        // The 503 can reach the client before the handler's commit reaches the gate.
        if state
            .block_on(workers::timeout(
                Duration::from_secs(10),
                gate.entered.wait(),
            ))
            .is_err()
        {
            return Err(Failure::operation(format!(
                "the commit answered {error:?} without reaching its gate"
            )));
        }
        Err(Failure::operation(error))
    }
}

impl Action for HoldCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// Releases the held commit and waits until it has its outcome.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCommit {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ReleaseCommit {
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
        let gate = state
            .node(self.common.node())?
            .spec
            .commit_gate
            .clone()
            .ok_or_else(|| Failure::operation("the node has no commit gate".to_owned()))?;
        gate.release();
        state
            .block_on(workers::timeout(Duration::from_secs(10), gate.ended.wait()))
            .map_err(|_| Failure::operation("the released commit never ended".to_owned()))?;
        Ok(json!({}))
    }
}

impl Action for ReleaseCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
