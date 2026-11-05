use crypto::{Key as _, PrivateKey as _};
use identity::Identity as _;
use serde_json::json;

use super::Node;

fn identity() -> identity::RawIdentity {
    identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap()
}

#[test]
fn native_node_admin_manages_foreign_relationships_and_rechecks_revocation() {
    edge::testing::edge().block_on(async {
        let admin = identity();
        let user = identity();
        let agent = identity();
        let outsider = identity();
        let admin_did = admin.did().unwrap().to_string();
        let user_did = user.did().unwrap().to_string();
        let agent_did = agent.did().unwrap().to_string();
        let outsider_did = outsider.did().unwrap().to_string();
        let node = Node::builder().enable_signing().with_node_acp_owner(admin_did.clone()).build().await.unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let context = |did: &str| defra_core::RequestContext::default().with_identity(Some(did.to_owned()));
        let admin_context = context(&admin_did);
        for did in [&node_did, &user_did, &agent_did, &outsider_did] {
            for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::DocumentUpdate,
                acp::nac::NodePermission::CollectionGet, acp::nac::NodePermission::DacRelationAdd,
                acp::nac::NodePermission::DacRelationDelete] {
                node.add_nac_actor_relationship_as(&admin_context, permission.as_str(), did).await.unwrap();
            }
        }
        let policy = node.add_dac_policy_as(&admin_context, "name: Admin fixture\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection(&format!("type AdminRecord @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let created = node.execute_request(query::QueryRequest::new("mutation($input:AdminRecordMutationInputArg!) { create_AdminRecord(input:$input) {_docID} }")
            .with_variables(json!({"input":{"name":"foreign"}})).with_identity(Some(user.did().unwrap()))).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_AdminRecord"][0]["_docID"].as_str().unwrap().to_owned();
        for did in [&node_did, &agent_did, &outsider_did] {
            let txn = node.begin_transaction(false).await.unwrap();
            assert!(node.add_dac_actor_relationship_for_in_txn(&txn, &context(did), "AdminRecord", &doc, "reader", "*").await.is_err());
            node.discard_transaction(&txn).await.unwrap();
        }
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacBypass.as_str(), &node_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(!node.add_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.unwrap());
        node.commit_transaction(&txn).await.unwrap();
        let visible = node.execute_request(query::QueryRequest::new("{ AdminRecord {_docID} }").with_identity(Some(outsider.did().unwrap()))).await;
        assert_eq!(visible.data.unwrap()["AdminRecord"].as_array().unwrap().len(), 1);
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.delete_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.unwrap());
        node.discard_transaction(&txn).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.delete_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.unwrap());
        node.delete_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacBypass.as_str(), &node_did).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err());
        let visible = node.execute_request(query::QueryRequest::new("{ AdminRecord {_docID} }").with_identity(Some(outsider.did().unwrap()))).await;
        assert_eq!(visible.data.unwrap()["AdminRecord"].as_array().unwrap().len(), 1);
        node.shutdown().await.unwrap();
    }).unwrap();
}
