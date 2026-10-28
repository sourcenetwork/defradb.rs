//! The operations every edge (HTTP, CLI, FFI, embedded, WASM) calls on a
//! database, so each is implemented once in the core.

mod error;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use defra_core::RequestContext;
use schema::CollectionVersion;

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
}
