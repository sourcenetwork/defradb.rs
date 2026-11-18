use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::Identity as _;
use serde_json::json;

use super::{EmbeddedSigner, Node};

fn identity() -> identity::RawIdentity {
    identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap()
}

fn signed_identity() -> (identity::RawIdentity, SigningConfig) {
    let private = crypto::generate_ed25519().unwrap();
    let public = private.public_key();
    let config = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: private.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    (
        identity::RawIdentity::from_private_key(private).unwrap(),
        config,
    )
}

fn exercise_native_admin() -> Vec<bool> {
    edge::testing::edge().block_on(async {
        let admin = identity();
        let (user, user_signing) = signed_identity();
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
        let created = node.execute_request(EmbeddedSigner::from_config(user_signing).unwrap().attach(query::QueryRequest::new("mutation($input:AdminRecordMutationInputArg!) { create_AdminRecord(input:$input) {_docID} }")
            .with_variables(json!({"input":{"name":"foreign"}})).with_identity(Some(user.did().unwrap())))).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.unwrap()["add_AdminRecord"][0]["_docID"].as_str().unwrap().to_owned();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(!node.add_dac_actor_relationship_for_in_txn(&txn, &context(&user_did), "AdminRecord", &doc, "reader", &agent_did).await.unwrap());
        node.commit_transaction(&txn).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.delete_dac_actor_relationship_for_in_txn(&txn, &context(&user_did), "AdminRecord", &doc, "reader", &agent_did).await.unwrap());
        node.commit_transaction(&txn).await.unwrap();
        let mut decisions = Vec::new();
        for did in [&node_did, &agent_did, &outsider_did] {
            let txn = node.begin_transaction(false).await.unwrap();
            let allowed = node.add_dac_actor_relationship_for_in_txn(&txn, &context(did), "AdminRecord", &doc, "reader", "*").await.is_ok();
            decisions.push(allowed);
            assert!(!allowed);
            node.discard_transaction(&txn).await.unwrap();
        }
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacBypass.as_str(), &node_did).await.unwrap();
        node.delete_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacRelationAdd.as_str(), &node_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let allowed = node.add_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.is_ok();
        decisions.push(allowed);
        assert!(!allowed);
        node.discard_transaction(&txn).await.unwrap();
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacRelationAdd.as_str(), &node_did).await.unwrap();
        node.disable_nac_as(&admin_context).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.add_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.is_err());
        node.discard_transaction(&txn).await.unwrap();
        node.re_enable_nac_as(&admin_context).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.add_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "owner", "*").await.is_err());
        node.discard_transaction(&txn).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let added = node.add_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await;
        decisions.push(added.is_ok());
        assert!(!added.unwrap());
        node.commit_transaction(&txn).await.unwrap();
        let visible = node.execute_request(query::QueryRequest::new("{ AdminRecord {_docID} }").with_identity(Some(outsider.did().unwrap()))).await;
        assert_eq!(visible.data.unwrap()["AdminRecord"].as_array().unwrap().len(), 1);
        let allowed = node.add_dac_actor_relationship(&node_did, "AdminRecord", &doc, "reader", &agent_did).await.is_ok();
        decisions.push(allowed);
        assert!(!allowed, "Go's public route admits only the document's managers");
        assert!(node.delete_dac_actor_relationship(&node_did, "AdminRecord", &doc, "reader", "*").await.is_err());
        node.delete_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacRelationDelete.as_str(), &node_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.delete_dac_actor_relationship_for_in_txn(&txn, &context(&node_did), "AdminRecord", &doc, "reader", "*").await.is_err());
        node.discard_transaction(&txn).await.unwrap();
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DacRelationDelete.as_str(), &node_did).await.unwrap();
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
        decisions
    }).unwrap()
}

#[test]
fn native_node_admin_manages_foreign_relationships_and_rechecks_revocation() {
    exercise_native_admin();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn administrative_relationship_admission_matches_executed_lean_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let decisions = exercise_native_admin();
    let inputs = decisions.iter().enumerate().map(|(index, _)| json!({
        "embedderRoute":index != 5, "actorMatches":true, "nacActive":true, "nacBound":true, "bypassAllowed":index >= 3,
        "operationAllowed":index != 3, "dacBound":true, "registered":true, "relationValid":true
    })).collect::<Vec<_>>();
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "AdministrativeRelationship.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let modeled: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (observed, expected) in decisions.into_iter().zip(modeled) {
        assert_eq!(observed, expected["mayAdmin"].as_bool().unwrap());
    }
}
