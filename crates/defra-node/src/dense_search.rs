//! Thin embedded-node wrappers over the generic db dense-search primitives.

use crate::Node;
use crate::Result;

pub use db::{DenseHybridSearchHit, DenseHybridSearchRequest, DenseHybridSearchResponse};

impl<S> Node<S>
where
    S: storage::corekv::Store + 'static,
{
    /// Embed free-form text using the node's configured embedding runtime.
    pub async fn embed_text(&self, text: &str, model: Option<&str>) -> Result<Vec<f64>> {
        Ok(db::embed_text(self.embedding_config(), text, model).await?)
    }

    /// Run dense-search v1 over an arbitrary collection.
    pub async fn hybrid_search_dense(
        &self,
        request: &DenseHybridSearchRequest,
    ) -> Result<DenseHybridSearchResponse> {
        Ok(db::hybrid_search_dense(self.runner.as_ref(), self.embedding_config(), request).await?)
    }
}

pub(crate) fn require_success(
    response: crate::QueryResponse,
    context: &str,
) -> Result<serde_json::Value> {
    Ok(db::require_query_success(response, context)?)
}
