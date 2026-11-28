use acp::NodePermission;
use identity::Did;

use crate::Node;

#[test]
fn schema_operations_use_the_supplied_actor() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_acp_owner("did:key:owner")
            .build()
            .await
            .unwrap();
        let owner_node = node.with_identity(Did::new("did:key:owner").unwrap());
        let actor_node = node.with_identity(Did::new("did:key:actor").unwrap());
        node.with_identity(Did::new("did:key:owner").unwrap())
            .add_collection("type Item { name: String }")
            .await
            .unwrap();
        assert!(actor_node
            .add_view("Item { name }", "type ItemView { name: String }", None)
            .await
            .is_err());
        assert!(actor_node.materialize_collection("Item").await.is_err());
        for permission in [NodePermission::ViewAdd, NodePermission::CollectionPatch] {
            owner_node
                .add_nac_actor_relationship(permission.as_str(), "did:key:actor")
                .await
                .unwrap();
        }
        actor_node
            .add_view("Item { name }", "type ItemView { name: String }", None)
            .await
            .unwrap();
        actor_node.materialize_collection("Item").await.unwrap();
        for permission in [NodePermission::ViewAdd, NodePermission::CollectionPatch] {
            owner_node
                .delete_nac_actor_relationship(permission.as_str(), "did:key:actor")
                .await
                .unwrap();
        }
        assert!(actor_node
            .add_view("Item { name }", "type OtherView { name: String }", None)
            .await
            .is_err());
        assert!(actor_node.materialize_collection("Item").await.is_err());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn an_actor_inherits_nothing_from_the_transaction_scope() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let caller = "did:key:caller";
        let actor = "did:key:actor";
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: vec![1; 32],
            public_key_hex: hex::encode([2; 32]),
            public_key_bytes: vec![2; 32],
            remote_signer: None,
            signing_authorization: None,
        };
        let txn = defra_core::TxnId { start: 0, route: 1 };
        for names_request in [false, true] {
            let scoped = node.with_identity(Did::new(caller).unwrap());
            let scoped = if names_request {
                scoped.with_request_delegation("caller-token".to_owned())
            } else {
                scoped.with_bearer(Some("caller-token".to_owned()))
            };
            let acting = scoped
                .with_signing(Some(signing.clone()))
                .with_dac_bypass(true)
                .with_transaction(txn)
                .acting_for(Did::new(actor).unwrap());

            let context = acting.actor.context(&node);
            assert_eq!(context.identity(), Some(actor));
            assert_eq!(context.bearer_token_for(actor), None);
            assert!(!context.bearer_names_request());
            assert!(context.signing().is_none());
            assert!(!context.dac_bypass());

            let context = acting
                .with_request_delegation("actor-token".to_owned())
                .actor
                .context(&node);
            assert_eq!(
                context.bearer_token_for(actor).as_deref(),
                Some("actor-token")
            );
            assert!(context.bearer_names_request());
        }
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
