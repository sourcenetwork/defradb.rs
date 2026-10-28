//! The embedded node as a [`client::Client`]: the in-process edge.

use std::sync::Arc;

use async_trait::async_trait;
use client::{GqlResult, Result};
use defra_core::RequestContext;
use identity::Did;
use query::QueryRequest;
use schema::CollectionVersion;

use crate::{EmbeddedNode, Error};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> client::Client for EmbeddedNode<S>
where
    S: storage::corekv::Store + 'static,
{
    async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        let (ops, context, sdl) = (
            Arc::clone(&self.schema_ops),
            context.clone(),
            sdl.to_owned(),
        );
        self.on_pool(move || async move { ops.add_collection(&context, &sdl).await })
            .await
            .map_err(|error| match error {
                Error::Client(error) => error,
                other => client::Error::Storage(other.to_string()),
            })
    }

    async fn exec_request(&self, context: &RequestContext, request: &str) -> Result<GqlResult> {
        let identity = context
            .identity()
            .map(Did::new)
            .transpose()
            .map_err(|e| client::Error::InvalidIdentity(e.to_string()))?;
        let response = self
            .execute_request_once(QueryRequest::new(request).with_identity(identity))
            .await;
        Ok(GqlResult {
            data: response.data.unwrap_or_default(),
            errors: response.errors.into_iter().map(|e| e.message).collect(),
        })
    }
}
