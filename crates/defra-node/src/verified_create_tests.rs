use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::Identity as _;
use serde_json::json;

use crate::{EmbeddedSigner, Node};

fn actor() -> (identity::RawIdentity, SigningConfig) {
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

#[test]
fn verified_user_create_shares_node_transaction_with_agent_effects() {
    const POLICY: &str = r#"
name: Verified create
resources:
  - name: records
    permissions:
      - name: read
      - name: update
      - name: delete
"#;
    edge::testing::edge().block_on(async {
        let (administrator, _) = actor();
        let (user, user_config) = actor();
        let (agent, agent_config) = actor();
        let admin_did = administrator.did().unwrap().to_string();
        let user_did = user.did().unwrap().to_string();
        let agent_did = agent.did().unwrap().to_string();
        let node = Node::builder().enable_signing().with_node_acp_owner(admin_did.clone()).build().await.unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let admin = defra_core::RequestContext::default().with_identity(Some(admin_did));
        for did in [&node_did, &user_did, &agent_did] {
            for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::DocumentUpdate, acp::nac::NodePermission::CollectionGet] {
                node.add_nac_actor_relationship_as(&admin, permission.as_str(), did).await.unwrap();
            }
        }
        let policy = node.add_dac_policy_as(&admin, POLICY).await.unwrap();
        node.add_collection(&format!("type Replacement @policy(id: \"{policy}\", resource: \"records\") {{ session: String @immutable, agent: String @immutable, content: String }}\ntype Marker @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let schema = node.get_collection("Replacement").unwrap().unwrap();
        let input = json!({"session":"session-doc", "agent":"agent-doc", "content":"approved"});
        let prepared = db::block::builder::ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &input, &user_config).unwrap();
        let evidence = prepared.evidence();
        let graph = db::block::builder::VerifiedClientCreate::verify(&storage::Crdts::default(), &schema, &input, evidence).unwrap();
        assert_eq!(graph.signer_did(), user_did);
        let signer = EmbeddedSigner::from_config(agent_config).unwrap();
        let marker = || query::QueryRequest::new("mutation { create_Marker(input:{name:\"effect\"}) {_docID} }")
            .with_identity(Some(identity::Did::new(&agent_did).unwrap()));
        let replacement = |actor: &str, value: serde_json::Value| query::QueryRequest::new(
            "mutation($input:ReplacementMutationInputArg!) { create_Replacement(input:$input) {_docID _version {cid}} }")
            .with_variables(json!({"input":value})).with_identity(Some(identity::Did::new(actor).unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn_with_signer(marker(), &txn, &signer).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let bad = node.execute_verified_create_in_txn(replacement(&user_did, json!({"session":"other", "agent":"agent-doc", "content":"approved"})), &txn, &graph).await;
        assert!(bad.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let observe = node.execute_request(query::QueryRequest::new("{ Marker {_docID} }")
            .with_identity(Some(identity::Did::new(&agent_did).unwrap()))).await;
        assert!(observe.data.unwrap()["Marker"].as_array().unwrap().is_empty());
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn_with_signer(marker(), &txn, &signer).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let wrong_actor = node.execute_verified_create_in_txn(replacement(&agent_did, input.clone()), &txn, &graph).await;
        assert!(wrong_actor.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn_with_signer(marker(), &txn, &signer).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let bypass = replacement(&user_did, input.clone()).with_context(
            defra_core::RequestContext::default().with_dac_bypass(true));
        assert!(node.execute_verified_create_in_txn(bypass, &txn, &graph).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.delete_nac_actor_relationship_as(&admin, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn_with_signer(marker(), &txn, &signer).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let denied = node.execute_verified_create_in_txn(replacement(&user_did, input.clone()), &txn, &graph).await;
        assert!(denied.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.add_nac_actor_relationship_as(&admin, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn_with_signer(marker(), &txn, &signer).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let result = node.execute_verified_create_in_txn(replacement(&user_did, input.clone()), &txn, &graph).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let row = &result.data.as_ref().unwrap()["add_Replacement"][0];
        assert_eq!(row["_docID"], graph.doc_id());
        let cid = row["_version"][0]["cid"].as_str().unwrap().to_owned();
        node.commit_transaction(&txn).await.unwrap();
        assert!(node.verified_block_signer_did(&cid).await.is_err());
        let (block_bytes, signature_bytes) = node
            .authorized_signed_block_bytes(&cid, Some(&user_did))
            .await
            .unwrap();
        let mut block = defra_core::block::Block::from_dag_cbor(&block_bytes).unwrap();
        let signature = defra_core::block::Signature::from_dag_cbor(&signature_bytes).unwrap();
        assert_eq!(block.signature.take(), Some(signature.generate_cid().unwrap()));
        let public = crypto::public_key_from_string(
            crypto::KeyType::Ed25519,
            &user_config.public_key_hex,
        )
        .unwrap();
        assert_eq!(public.did().unwrap(), user_did);
        assert!(public.verify(&block.to_dag_cbor().unwrap(), &signature.value).unwrap());
        let mine = node.execute_request(query::QueryRequest::new("{ Replacement {_docID} }")
            .with_identity(Some(identity::Did::new(&user_did).unwrap()))).await;
        assert_eq!(mine.data.unwrap()["Replacement"][0]["_docID"], graph.doc_id());
        let other = node.execute_request(query::QueryRequest::new("{ Replacement {_docID} }")
            .with_identity(Some(identity::Did::new(&agent_did).unwrap()))).await;
        assert!(other.data.unwrap()["Replacement"].as_array().unwrap().is_empty());
        node.shutdown().await.unwrap();
    }).unwrap();
}
