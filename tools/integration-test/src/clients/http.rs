use std::fmt::Debug;
use std::future::Future;

use cli::commands::client::http_client::HttpClient;
use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::client_type::network_edge;
use crate::scenario::identity::private_key_hex;
use crate::scenario::Failure;
use crate::test_case::State;

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
}
