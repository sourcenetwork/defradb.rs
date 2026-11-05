use std::sync::Arc;

use defra_core::RequestContext;

use crate::{Node, Result};

impl Node {
    /// Create a view as the explicitly authenticated caller.
    pub async fn add_view_as(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
    ) -> Result<()> {
        let ops = Arc::clone(&self.schema_ops);
        let context = context.clone();
        let query = source_query.to_owned();
        let sdl = target_sdl.to_owned();
        self.on_pool(move || async move { ops.add_view(&context, &query, &sdl).await })
            .await
    }

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
mod tests {
    use super::*;
    use acp::NodePermission;

    #[test]
    fn schema_operations_use_the_supplied_actor() {
        workers::block_on(async {
            let node = Node::builder()
                .with_node_acp_owner("did:key:owner")
                .build()
                .await
                .unwrap();
            node.add_collection("type Item { name: String }")
                .await
                .unwrap();
            let owner = RequestContext::default().with_identity(Some("did:key:owner".into()));
            let actor = RequestContext::default().with_identity(Some("did:key:actor".into()));
            assert!(node
                .add_view_as(&actor, "Item { name }", "type ItemView { name: String }")
                .await
                .is_err());
            assert!(node
                .materialize_collection_as(&actor, "Item")
                .await
                .is_err());
            for permission in [NodePermission::ViewAdd, NodePermission::CollectionPatch] {
                node.add_nac_actor_relationship_as(&owner, permission.as_str(), "did:key:actor")
                    .await
                    .unwrap();
            }
            node.add_view_as(&actor, "Item { name }", "type ItemView { name: String }")
                .await
                .unwrap();
            node.materialize_collection_as(&actor, "Item")
                .await
                .unwrap();
            for permission in [NodePermission::ViewAdd, NodePermission::CollectionPatch] {
                node.delete_nac_actor_relationship_as(&owner, permission.as_str(), "did:key:actor")
                    .await
                    .unwrap();
            }
            assert!(node
                .add_view_as(&actor, "Item { name }", "type OtherView { name: String }")
                .await
                .is_err());
            assert!(node
                .materialize_collection_as(&actor, "Item")
                .await
                .is_err());
            node.shutdown().await.unwrap();
        })
        .unwrap();
    }
}
