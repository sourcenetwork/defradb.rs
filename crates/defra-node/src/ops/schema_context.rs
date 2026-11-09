use std::sync::Arc;

use defra_core::RequestContext;

use crate::{Node, Result};

impl Node {
    /// Materialize a collection as the explicitly authenticated caller.
    pub async fn materialize_collection_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
    ) -> Result<usize> {
        let ops = Arc::clone(&self.schema_ops);
        let context = context.clone();
        let name = collection_name.to_owned();
        self.on_pool(move || async move { ops.materialize_collection(&context, &name).await })
            .await
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ops_schema_context.rs"]
mod tests;
