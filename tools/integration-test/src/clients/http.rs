use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

use cli::commands::client::http_client::HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

use crate::client_type::network_edge;
use crate::scenario::identity::private_key_hex;
use crate::scenario::Failure;
use crate::test_case::State;

const INDEX_BUILD_TIMEOUT: Duration = Duration::from_secs(20);
const INDEX_BUILD_POLL: Duration = Duration::from_millis(10);

/// Go's `BackfillIndexAction` with `InProgressActionStatus`: an index build.
const BACKFILL_INDEX_ACTION: u64 = 3;
const IN_PROGRESS_STATUS: u64 = 1;

/// An action's body for the HTTP clients.
pub trait HttpRun {
    fn execute_http(&self, state: &mut State);
}

/// `A` run through the node's HTTP API.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Http<A>(pub A);

impl<A> Action for Http<A>
where
    A: HttpRun + Clone + Debug + Serialize + 'static,
{
    type TState = State;

    fn execute(&self, state: &mut State) {
        self.0.execute_http(state);
    }
}

impl State {
    /// Makes the request `call` builds with a client of node `name`'s HTTP
    /// API. It runs on the network edge, and the test waits for it on a
    /// worker, which has no tokio context.
    pub fn http_on<F, Fut, T>(&self, name: &str, call: F) -> Result<T, Failure>
    where
        F: FnOnce(HttpClient) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.http_as(name, None, call)
    }

    /// [`State::http_on`] as client identity `identity` when the action
    /// names one: its requests carry a bearer token for the node.
    pub fn http_as<F, Fut, T>(
        &self,
        name: &str,
        identity: Option<u64>,
        call: F,
    ) -> Result<T, Failure>
    where
        F: FnOnce(HttpClient) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let url = self.url_on(name)?;
        let token = identity.map(|seed| {
            cli::commands::client::generate_auth_token(&private_key_hex(seed), &url)
                .expect("a token for a test identity")
        });
        let client = HttpClient::new(&url)
            .expect("http client")
            .with_auth_token(token);
        Ok(self
            .block_on(network_edge().spawn(call(client)))
            .expect("run on the network edge"))
    }

    /// Waits until node `name` is building no index, as Go's suite does after
    /// a patch, a version switch or a migration outside a transaction
    /// (`WaitForNodeIndexesBuilt`). Go rebuilds indexes in the background, and
    /// a query that races the build fails with a transaction conflict. Only a
    /// valid listing with no build in progress counts as built; a failed or
    /// unreadable listing is retried until the deadline. A node whose listing
    /// reports no build status has nothing to wait for.
    pub fn wait_for_indexes_built(&self, name: &str) -> Result<(), Failure> {
        let url = format!("{}/api/v0/collections/indexes", self.url_on(name)?);
        let outcome = self
            .block_on(network_edge().spawn(async move {
                let http = reqwest::Client::new();
                let deadline = tokio::time::Instant::now() + INDEX_BUILD_TIMEOUT;
                loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    let listing = match http.get(&url).timeout(remaining).send().await {
                        Ok(response) => response.json::<Value>().await.map_err(|e| e.to_string()),
                        Err(error) => Err(error.to_string()),
                    };
                    let outcome = match &listing {
                        Ok(listing) if !any_index_building(listing) => return Ok(()),
                        Ok(_) => "an index is still building".to_owned(),
                        Err(error) => format!("the index listing failed: {error}"),
                    };
                    if tokio::time::Instant::now() >= deadline {
                        return Err(outcome);
                    }
                    tokio::time::sleep(INDEX_BUILD_POLL).await;
                }
            }))
            .expect("run on the network edge");
        outcome.map_err(|reason| {
            Failure::operation(format!(
                "node {name} after {INDEX_BUILD_TIMEOUT:?}: {reason}"
            ))
        })
    }
}

/// Whether a `GET /collections/indexes` listing (collection name to its
/// indexes, each with its `Execution`) holds an index still being built.
fn any_index_building(listing: &Value) -> bool {
    let Some(collections) = listing.as_object() else {
        return false;
    };
    collections
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .any(|index| {
            let execution = &index["Execution"];
            execution["Action"].as_u64() == Some(BACKFILL_INDEX_ACTION)
                && execution["Status"].as_u64() == Some(IN_PROGRESS_STATUS)
        })
}
