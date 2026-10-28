use std::fmt::Debug;
use std::future::Future;

use cli::commands::client::http_client::HttpClient;
use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::client_type::network_edge;
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
    /// Makes the request `call` builds with a client of the node's HTTP API.
    /// It runs on the network edge, and the test waits for it on a worker,
    /// which has no tokio context.
    pub fn http<F, Fut, T>(&self, call: F) -> T
    where
        F: FnOnce(HttpClient) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let client = HttpClient::new(self.url()).expect("http client");
        self.block_on(network_edge().spawn(call(client)))
            .expect("run on the network edge")
    }
}
