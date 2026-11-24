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
        let owner = RequestContext::default().with_identity(Some("did:key:owner".into()));
        let actor = RequestContext::default().with_identity(Some("did:key:actor".into()));
        node.add_collection_as(&owner, "type Item { name: String }")
            .await
            .unwrap();
        assert!(node
            .add_view_as(
                &actor,
                "Item { name }",
                "type ItemView { name: String }",
                None
            )
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
        node.add_view_as(
            &actor,
            "Item { name }",
            "type ItemView { name: String }",
            None,
        )
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
            .add_view_as(
                &actor,
                "Item { name }",
                "type OtherView { name: String }",
                None
            )
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
