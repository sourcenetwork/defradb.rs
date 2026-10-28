//! The operations every edge (HTTP, CLI, FFI, embedded, WASM) calls on a
//! database, so each is implemented once in the core.

mod error;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use defra_core::RequestContext;
use schema::CollectionVersion;

/// The outcome of a GraphQL request, as Go's `client.GQLResult`: the response
/// data and the message of each error.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GqlResult {
    pub data: serde_json::Value,
    pub errors: Vec<String>,
}

pub use error::{Error, Result};

/// The database as an edge sees it.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Client: MaybeSendSync {
    /// Parse `sdl`, check it and create every collection it defines, or none.
    async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>>;

    /// Run a GraphQL request. Errors in the request are in the result, not
    /// the `Err`, which is for a request that could not be made.
    async fn exec_request(&self, context: &RequestContext, request: &str) -> Result<GqlResult>;
}
