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
        let outcome = match state.client_type() {
            ClientType::RustHttp => {
                let url = state.url_on(self.common.node())?;
                state
                    .block_on(network_edge().spawn(async move {
                        let token = identity.map(|seed| {
                            cli::commands::client::generate_auth_token(
                                &crate::scenario::identity::private_key_hex(seed),
                                &url,
                            )
                            .expect("a token for a test identity")
                        });
                        let client = net::http::Client::new(network_edge());
                        let request = client
                            .post(format!("{url}/api/v0/tx/{id}"))
                            .header(
                                "Authorization",
                                token
                                    .map(|token| format!("Bearer {token}"))
                                    .unwrap_or_default(),
                            )
                            .body("");
                        let commit = nuclei::spawn(async move { request.send().await });
                        if nuclei::time::timeout(Duration::from_secs(10), gate.entered.wait())
                            .await
                            .is_err()
                        {
                            return Err(net::http::Error::Request {
                                url: format!("{url}/api/v0/tx/{id}"),
                                source: Box::new(std::io::Error::other(
                                    "the commit never reached its gate",
                                )),
                            });
                        }
                        commit.join().await.unwrap()
                    }))
                    .map_err(|error| Failure::operation(error.to_string()))?
            }
            ClientType::Cli => {
                let printed = state.defra_client_as(
                    self.common.node(),
                    None,
                    identity,
                    &["tx", "commit", &id],
                );
                return match printed {
                    Ok(value) => Err(Failure::operation(format!(
                        "the bounded commit unexpectedly succeeded: {value}"
                    ))),
                    Err(error) => {
                        if !error.contains(&self.expected_error) {
                            return Err(Failure::operation(format!(
                                "bounded commit error {error:?} did not contain {:?}",
                                self.expected_error
                            )));
                        }
                        Ok(json!({}))
                    }
                };
            }
            client => {
                return Err(Failure::unsupported(format!(
                    "{} under {client:?}",
                    Self::name()
                )))
            }
        };
        let response = outcome.map_err(|error| Failure::operation(error.to_string()))?;
        if response.status().as_u16() != 503 {
            return Err(Failure::operation(format!(
                "the bounded commit answered {}",
                response.status()
            )));
        }
        let body = state
            .block_on(async { response.text().await })
            .map_err(|error| Failure::operation(error.to_string()))?;
        let value: Value = serde_json::from_str(&body).map_err(|error| {
            Failure::operation(format!("bad bounded-commit body {body:?}: {error}"))
        })?;
        if value["outcome"] != json!("unknown")
            || !value["error"]
                .as_str()
                .is_some_and(|error| error.contains(&self.expected_error))
        {
            return Err(Failure::operation(format!(
                "unexpected bounded-commit body: {value}"
            )));
        }
        Ok(json!({}))
    }
}

impl Action for HoldCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// Releases the held commit and gives its worker a moment to finish it.
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
        std::thread::sleep(Duration::from_millis(20));
        Ok(json!({}))
    }
}

impl Action for ReleaseCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
