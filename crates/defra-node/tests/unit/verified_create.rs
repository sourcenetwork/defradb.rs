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

async fn assert_user_signed_block(node: &Node, cid: &str, user_did: &str, signing: &SigningConfig) {
    let (block_bytes, signature_bytes) = node
        .authorized_signed_block_bytes(cid, Some(user_did))
        .await
        .unwrap();
    let mut block = defra_core::block::Block::from_dag_cbor(&block_bytes).unwrap();
    let signature = defra_core::block::Signature::from_dag_cbor(&signature_bytes).unwrap();
    assert_eq!(block.generate_cid().unwrap().to_string(), cid);
    assert_eq!(
        block.signature.take(),
        Some(signature.generate_cid().unwrap())
    );
    assert_eq!(
        signature.header.sig_type,
        defra_core::block::SignatureType::EdDSA
    );
    assert_eq!(
        signature.header.identity.as_slice(),
        signing.public_key_hex.as_bytes()
    );
    let public =
        crypto::public_key_from_string(crypto::KeyType::Ed25519, &signing.public_key_hex).unwrap();
    assert_eq!(public.did().unwrap(), user_did);
    assert!(public
        .verify(&block.to_dag_cbor().unwrap(), &signature.value)
        .unwrap());
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
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let bad = node.execute_verified_create_in_txn(replacement(&user_did, json!({"session":"other", "agent":"agent-doc", "content":"approved"})), &txn, &graph).await;
        assert!(bad.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let observe = node.execute_request(query::QueryRequest::new("{ Marker {_docID} }")
            .with_identity(Some(identity::Did::new(&agent_did).unwrap()))).await;
        assert!(observe.data.unwrap()["Marker"].as_array().unwrap().is_empty());
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let parsed = query::query_parse::parse_request_with_limits(
            "mutation { create_Marker(input:{name:\"substituted\"}) {_docID} }", None, None,
            query::QueryLimits::default()).unwrap();
        let mut mismatched = replacement(&user_did, input.clone());
        mismatched.parsed = Some(std::sync::Arc::new(parsed));
        assert!(node.execute_verified_create_in_txn(mismatched, &txn, &graph).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let wrong_actor = node.execute_verified_create_in_txn(replacement(&agent_did, input.clone()), &txn, &graph).await;
        assert!(wrong_actor.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let bypass = replacement(&user_did, input.clone()).with_context(
            defra_core::RequestContext::default().with_dac_bypass(true));
        assert!(node.execute_verified_create_in_txn(bypass, &txn, &graph).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.delete_nac_actor_relationship_as(&admin, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let denied = node.execute_verified_create_in_txn(replacement(&user_did, input.clone()), &txn, &graph).await;
        assert!(denied.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.add_nac_actor_relationship_as(&admin, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let first = node.execute_request_in_txn(signer.attach(marker()), &txn).await;
        assert!(!first.has_errors(), "{:?}", first.errors);
        let result = node.execute_verified_create_in_txn(replacement(&user_did, input.clone()), &txn, &graph).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let row = &result.data.as_ref().unwrap()["add_Replacement"][0];
        assert_eq!(row["_docID"], graph.doc_id());
        let cid = row["_version"][0]["cid"].as_str().unwrap().to_owned();
        node.commit_transaction(&txn).await.unwrap();
        assert!(node.verified_block_signer_did(&cid).await.is_err());
        assert_user_signed_block(&node, &cid, &user_did, &user_config).await;
        let txn = node.begin_transaction(false).await.unwrap();
        let next_marker = query::QueryRequest::new("mutation { create_Marker(input:{name:\"must-rollback\"}) {_docID} }")
            .with_identity(Some(agent.did().unwrap()));
        let staged = node.execute_request_in_txn(signer.attach(next_marker), &txn).await;
        assert!(!staged.has_errors(), "{:?}", staged.errors);
        let duplicate = node.execute_verified_create_in_txn(replacement(&user_did, input.clone()), &txn, &graph).await;
        assert!(duplicate.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let markers = node.execute_request(query::QueryRequest::new("{ Marker {name} }")
            .with_identity(Some(agent.did().unwrap()))).await;
        assert_eq!(markers.data.unwrap()["Marker"].as_array().unwrap().len(), 1);
        let mine = node.execute_request(query::QueryRequest::new("{ Replacement {_docID} }")
            .with_identity(Some(identity::Did::new(&user_did).unwrap()))).await;
        assert_eq!(mine.data.unwrap()["Replacement"][0]["_docID"], graph.doc_id());
        let other = node.execute_request(query::QueryRequest::new("{ Replacement {_docID} }")
            .with_identity(Some(identity::Did::new(&agent_did).unwrap()))).await;
        assert!(other.data.unwrap()["Replacement"].as_array().unwrap().is_empty());
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn verified_update_and_delete_use_exact_actor_and_transaction_state() {
    use db::block::builder::{
        ClientCreateSignatures, ClientDeleteSignatures, ClientUpdateSignatures,
        ClientUpdateSnapshot, VerifiedClientMutation,
    };
    const POLICY: &str = "name: Mutations\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n      - name: editor\n        types: [actor]\n      - name: deleter\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader + editor + deleter\n      - name: update\n        expr: editor\n      - name: delete\n        expr: deleter\n";
    edge::testing::edge().block_on(async {
        let (admin, _) = actor();
        let (user, signing) = actor();
        let (other, other_signing) = actor();
        let (agent, agent_signing) = actor();
        let admin_did = admin.did().unwrap().to_string();
        let user_did = user.did().unwrap().to_string();
        let other_did = other.did().unwrap().to_string();
        let agent_did = agent.did().unwrap().to_string();
        let node = Node::builder().enable_signing().with_node_acp_owner(admin_did.clone()).build().await.unwrap();
        let admin_context = defra_core::RequestContext::default().with_identity(Some(admin_did));
        let node_did = node.node_identity_did().unwrap().to_owned();
        for did in [&node_did, &user_did, &other_did, &agent_did] {
            for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::DocumentUpdate,
                acp::nac::NodePermission::DocumentDelete, acp::nac::NodePermission::CollectionGet] {
                node.add_nac_actor_relationship_as(&admin_context, permission.as_str(), did).await.unwrap();
            }
        }
        for permission in [acp::nac::NodePermission::DacRelationAdd, acp::nac::NodePermission::DacRelationDelete] {
            node.add_nac_actor_relationship_as(&admin_context, permission.as_str(), &user_did).await.unwrap();
        }
        let policy = node.add_dac_policy_as(&admin_context, POLICY).await.unwrap();
        node.add_collection(&format!("type VerifiedRecord @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}\ntype VerifiedMarker @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let schema = node.get_collection("VerifiedRecord").unwrap().unwrap();
        let crdts = storage::Crdts::default();
        let input = json!({"name":"before"});
        let create = ClientCreateSignatures::from_input(&crdts, &schema, &input, &signing).unwrap();
        let create = db::block::builder::VerifiedClientCreate::verify(&crdts, &schema, &input, create.evidence()).unwrap();
        let request = query::QueryRequest::new("mutation($input:VerifiedRecordMutationInputArg!) { create_VerifiedRecord(input:$input) {_docID} }")
            .with_variables(json!({"input":input})).with_identity(Some(user.did().unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_create_in_txn(request, &txn, &create).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        node.commit_transaction(&txn).await.unwrap();
        let doc_id = create.doc_id().to_owned();
        let history_request = || query::QueryRequest::new("query($docID:[String!]!) { _commits(docID:$docID) { cid docID fieldName height delta collectionVersionId links { cid fieldName } heads { cid } signature { type identity value } } }")
            .with_variables(json!({"docID":[doc_id]})).with_identity(Some(user.did().unwrap()));
        let history = node.execute_request(history_request()).await;
        assert!(!history.has_errors(), "{:?}", history.errors);
        let snapshot = ClientUpdateSnapshot::from_commits(&doc_id, history.data.unwrap()["_commits"].as_array().unwrap()).unwrap();
        let patch = json!({"name":"after"});
        let prepared = ClientUpdateSignatures::from_input(&crdts, &schema, &doc_id, &patch, &snapshot, &signing).unwrap();
        let mut update_block = defra_core::block::Block::from_dag_cbor(prepared.composite_block()).unwrap();
        update_block.signature = Some(db::block::builder::compute_signature(&update_block, &signing).unwrap().unwrap().0);
        let update_cid = update_block.generate_cid().unwrap().to_string();
        let update = VerifiedClientMutation::verify_update(&crdts, &schema, &doc_id, &patch, &snapshot, prepared.evidence()).unwrap();
        let stale_delete = ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, &signing).unwrap();
        let stale_delete = VerifiedClientMutation::verify_delete(&schema, &doc_id, &snapshot, stale_delete.evidence()).unwrap();
        let update_request = |did: &str| query::QueryRequest::new("mutation($id:ID!, $input:VerifiedRecordMutationInputArg!) { update_VerifiedRecord(docID:$id,input:$input) {_docID} }")
            .with_variables(json!({"id":doc_id,"input":patch})).with_identity(Some(identity::Did::new(did).unwrap()));
        let user_context = defra_core::RequestContext::default().with_identity(Some(user_did.clone()));
        node.add_dac_actor_relationship_as(&user_context, "VerifiedRecord", &doc_id, "reader", &other_did).await.unwrap();
        node.add_dac_actor_relationship_as(&user_context, "VerifiedRecord", &doc_id, "editor", &other_did).await.unwrap();
        let other_graph = ClientUpdateSignatures::from_input(&crdts, &schema, &doc_id, &patch, &snapshot, &other_signing).unwrap();
        let other_graph = VerifiedClientMutation::verify_update(&crdts, &schema, &doc_id, &patch, &snapshot, other_graph.evidence()).unwrap();
        node.delete_dac_actor_relationship_as(&user_context, "VerifiedRecord", &doc_id, "editor", &other_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.execute_verified_mutation_in_txn(update_request(&other_did), &txn, &other_graph).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.execute_verified_mutation_in_txn(update_request(&other_did), &txn, &update).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.delete_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.execute_verified_mutation_in_txn(update_request(&user_did), &txn, &update).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DocumentUpdate.as_str(), &user_did).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_mutation_in_txn(update_request(&user_did), &txn, &update).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        node.commit_transaction(&txn).await.unwrap();
        assert_user_signed_block(&node, &update_cid, &user_did, &signing).await;
        let agent_signer = EmbeddedSigner::from_config(agent_signing).unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let marker = query::QueryRequest::new("mutation { create_VerifiedMarker(input:{name:\"must-rollback\"}) {_docID} }")
            .with_identity(Some(agent.did().unwrap()));
        let staged = node.execute_request_in_txn(agent_signer.attach(marker), &txn).await;
        assert!(!staged.has_errors(), "{:?}", staged.errors);
        let stale_equal = node.execute_verified_mutation_in_txn(update_request(&user_did), &txn, &update).await;
        assert!(stale_equal.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let marker = node.execute_request(query::QueryRequest::new("{ VerifiedMarker {_docID} }")
            .with_identity(Some(agent.did().unwrap()))).await;
        assert!(marker.data.unwrap()["VerifiedMarker"].as_array().unwrap().is_empty());
        let delete_request = || query::QueryRequest::new("mutation($id:ID!) { delete_VerifiedRecord(docIDs:[$id]) {_docID} }")
            .with_variables(json!({"id":doc_id})).with_identity(Some(user.did().unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(node.execute_verified_mutation_in_txn(delete_request(), &txn, &stale_delete).await.has_errors());
        assert!(node.commit_transaction(&txn).await.is_err());
        let history = node.execute_request(history_request()).await;
        let snapshot = ClientUpdateSnapshot::from_commits(&doc_id, history.data.unwrap()["_commits"].as_array().unwrap()).unwrap();
        let prepared = ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, &signing).unwrap();
        let mut delete_block = defra_core::block::Block::from_dag_cbor(prepared.composite_block()).unwrap();
        delete_block.signature = Some(db::block::builder::compute_signature(&delete_block, &signing).unwrap().unwrap().0);
        let delete_cid = delete_block.generate_cid().unwrap().to_string();
        let delete = VerifiedClientMutation::verify_delete(&schema, &doc_id, &snapshot, prepared.evidence()).unwrap();
        let marker = |name: &str| query::QueryRequest::new("mutation($input:VerifiedMarkerMutationInputArg!) { create_VerifiedMarker(input:$input) {_docID} }")
            .with_variables(json!({"input":{"name":name}}))
            .with_identity(Some(agent.did().unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(!node.execute_request_in_txn(agent_signer.attach(marker("nac-revoked")), &txn).await.has_errors());
        let staged = node.execute_verified_mutation_in_txn(delete_request(), &txn, &delete).await;
        assert!(!staged.has_errors(), "{:?}", staged.errors);
        node.delete_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DocumentDelete.as_str(), &user_did).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err());
        node.add_nac_actor_relationship_as(&admin_context, acp::nac::NodePermission::DocumentDelete.as_str(), &user_did).await.unwrap();
        node.add_dac_actor_relationship_as(&user_context, "VerifiedRecord", &doc_id, "deleter", &other_did).await.unwrap();
        let prepared = ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, &other_signing).unwrap();
        let other_delete = VerifiedClientMutation::verify_delete(&schema, &doc_id, &snapshot, prepared.evidence()).unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert!(!node.execute_request_in_txn(agent_signer.attach(marker("dac-revoked")), &txn).await.has_errors());
        let request = query::QueryRequest::new("mutation($id:ID!) { delete_VerifiedRecord(docIDs:[$id]) {_docID} }")
            .with_variables(json!({"id":doc_id})).with_identity(Some(other.did().unwrap()));
        let staged = node.execute_verified_mutation_in_txn(request, &txn, &other_delete).await;
        assert!(!staged.has_errors(), "{:?}", staged.errors);
        node.delete_dac_actor_relationship_as(&user_context, "VerifiedRecord", &doc_id, "deleter", &other_did).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err());
        let markers = node.execute_request(query::QueryRequest::new("{ VerifiedMarker {_docID} }")
            .with_identity(Some(agent.did().unwrap()))).await;
        assert!(markers.data.unwrap()["VerifiedMarker"].as_array().unwrap().is_empty());
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_mutation_in_txn(delete_request(), &txn, &delete).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        node.discard_transaction(&txn).await.unwrap();
        let retained = node.execute_request(query::QueryRequest::new("{ VerifiedRecord {_docID name} }").with_identity(Some(user.did().unwrap()))).await;
        assert_eq!(retained.data.unwrap()["VerifiedRecord"][0]["name"], "after");
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_mutation_in_txn(delete_request(), &txn, &delete).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        node.commit_transaction(&txn).await.unwrap();
        assert_user_signed_block(&node, &delete_cid, &user_did, &signing).await;
        let retained = node.execute_request(query::QueryRequest::new("{ VerifiedRecord {_docID} }").with_identity(Some(user.did().unwrap()))).await;
        assert!(retained.data.unwrap()["VerifiedRecord"].as_array().unwrap().is_empty());
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// Shutdown stops admitting requests before P2P stops recording head markers,
/// so a write signed with a caller's key or a client's graph must stop there too.
#[test]
fn signed_writes_are_refused_once_requests_have_drained() {
    edge::testing::edge().block_on(async {
        let (agent, agent_config) = actor();
        let (user, signing) = actor();
        let node = Node::builder().enable_signing().build().await.unwrap();
        node.add_collection("type DrainedRecord { name: String }").await.unwrap();
        let schema = node.get_collection("DrainedRecord").unwrap().unwrap();
        let input = json!({"name":"late"});
        let crdts = storage::Crdts::default();
        let signed = db::block::builder::ClientCreateSignatures::from_input(&crdts, &schema, &input, &signing).unwrap();
        let graph = db::block::builder::VerifiedClientCreate::verify(&crdts, &schema, &input, signed.evidence()).unwrap();
        let signer = EmbeddedSigner::from_config(agent_config).unwrap();
        let txn = node.begin_transaction(false).await.unwrap();

        node.network.as_ref().unwrap().drain_requests().await;

        let caller_signed = node.execute_request(signer.attach(
            query::QueryRequest::new("mutation { create_DrainedRecord(input:{name:\"caller\"}) {_docID} }")
                .with_identity(Some(agent.did().unwrap())),
        )).await;
        let client_signed = node.execute_verified_create_in_txn(
            query::QueryRequest::new("mutation($input:DrainedRecordMutationInputArg!) { create_DrainedRecord(input:$input) {_docID} }")
                .with_variables(json!({"input":input})).with_identity(Some(user.did().unwrap())),
            &txn,
            &graph,
        ).await;
        for response in [caller_signed, client_signed] {
            assert_eq!(
                response.errors.first().map(|error| error.message.as_str()),
                Some("the node is shutting down")
            );
        }
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn verified_create_allows_disabled_nac_and_unbound_collection() {
    edge::testing::edge().block_on(async {
        let (user, signing) = actor();
        let node = Node::builder().enable_signing().build().await.unwrap();
        node.add_collection("type PlainVerifiedRecord { name: String }").await.unwrap();
        let schema = node.get_collection("PlainVerifiedRecord").unwrap().unwrap();
        let input = json!({"name":"plain"});
        let crdts = storage::Crdts::default();
        let signed = db::block::builder::ClientCreateSignatures::from_input(&crdts, &schema, &input, &signing).unwrap();
        let graph = db::block::builder::VerifiedClientCreate::verify(&crdts, &schema, &input, signed.evidence()).unwrap();
        let request = query::QueryRequest::new("mutation($input:PlainVerifiedRecordMutationInputArg!) { create_PlainVerifiedRecord(input:$input) {_docID} }")
            .with_variables(json!({"input":input})).with_identity(Some(user.did().unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_create_in_txn(request, &txn, &graph).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        node.commit_transaction(&txn).await.unwrap();
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// An identity-only node never signs, so a verified create is signed only by
/// its caller, and it runs in a transaction the node began for no one.
#[test]
fn verified_create_on_an_identity_only_node_is_signed_by_its_caller() {
    edge::testing::edge().block_on(async {
        let (user, signing) = actor();
        let user_did = user.did().unwrap().to_string();
        let node = Node::builder().enable_signing().with_identity_only().build().await.unwrap();
        node.add_collection("type PlainVerifiedRecord { name: String }").await.unwrap();
        let schema = node.get_collection("PlainVerifiedRecord").unwrap().unwrap();
        let input = json!({"name":"identity-only"});
        let crdts = storage::Crdts::default();
        let signed = db::block::builder::ClientCreateSignatures::from_input(&crdts, &schema, &input, &signing).unwrap();
        let graph = db::block::builder::VerifiedClientCreate::verify(&crdts, &schema, &input, signed.evidence()).unwrap();
        let request = query::QueryRequest::new("mutation($input:PlainVerifiedRecordMutationInputArg!) { create_PlainVerifiedRecord(input:$input) {_docID _version {cid}} }")
            .with_variables(json!({"input":input})).with_identity(Some(user.did().unwrap()));
        let txn = node.begin_transaction(false).await.unwrap();
        let result = node.execute_verified_create_in_txn(request, &txn, &graph).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let row = &result.data.as_ref().unwrap()["add_PlainVerifiedRecord"][0];
        assert_eq!(row["_docID"], graph.doc_id());
        let cid = row["_version"][0]["cid"].as_str().unwrap().to_owned();
        node.commit_transaction(&txn).await.unwrap();
        assert_user_signed_block(&node, &cid, &user_did, &signing).await;
        node.shutdown().await.unwrap();
    }).unwrap();
}
