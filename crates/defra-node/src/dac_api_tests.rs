use super::tests::SIGNING_STORE_GUARD;
use super::Node;
use crypto::Key;
use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Identity as _, RawIdentity};

const POLICY_YAML: &str = r#"
name: Sensitive Rows
resources:
  - name: users
    relations:
      - name: reader
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;
const DID_A: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const DID_B: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

async fn node_with_protected_users() -> (Node, String) {
    let node = Node::builder().build().await.unwrap();
    let policy_id = node
        .add_dac_policy_as(
            &defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))),
            POLICY_YAML,
        )
        .await
        .unwrap();
    node.add_collection(&format!(
        "type Users @policy(id: \"{policy_id}\", resource: \"users\") {{ name: String }}"
    ))
    .await
    .unwrap();

    let did_a = identity::Did::new(DID_A).unwrap();
    let create = node
        .execute_request(
            query::QueryRequest::new(
                "mutation { add_Users(input: {name: \"secret\"}) { _docID } }",
            )
            .with_identity(Some(did_a)),
        )
        .await;
    assert!(!create.has_errors(), "{:?}", create.errors);
    let doc_id = create.data.as_ref().unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    (node, doc_id)
}

/// Generate a fresh Ed25519 node identity and register it in the process-local
/// signing registry with exportable key bytes, so `with_node_acp_enabled` can
/// match it against the database node DID.
fn register_local_node_identity() -> String {
    let private_key = crypto::generate_ed25519().unwrap();
    let identity = RawIdentity::from_ed25519(private_key.clone()).unwrap();
    let did = identity.did().unwrap().to_string();

    let public_key_bytes = identity.public_key_bytes();
    defra_core::signing::store_identity(
        &did,
        SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: private_key.raw().to_vec(),
            public_key_bytes: public_key_bytes.clone(),
            public_key_hex: hex::encode(&public_key_bytes),
            remote_signer: None,
            signing_authorization: None,
        },
    );

    did
}

async fn visible_users(node: &Node, identity: Option<identity::Did>) -> usize {
    let response = node
        .execute_request(
            query::QueryRequest::new("query { Users { name } }").with_identity(identity),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["Users"].as_array().unwrap().len()
}

#[test]
fn deleted_protected_document_does_not_expose_historical_commits_or_blocks() {
    workers::block_on(async {
        let node = Node::builder().enable_signing().build().await.unwrap();
        let owner = identity::Did::new(DID_A).unwrap();
        let foreign = identity::Did::new(DID_B).unwrap();
        let policy_id = node
            .add_dac_policy_as(
                &defra_core::RequestContext::default().with_identity(Some(DID_A.to_owned())),
                POLICY_YAML,
            )
            .await
            .unwrap();
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy_id}\", resource: \"users\") {{ name: String }}"
        ))
        .await
        .unwrap();
        let created = node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Users(input: {name: \"secret\"}) { _docID } }",
                )
                .with_identity(Some(owner.clone())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc_id = created.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let query = format!(
            r#"query {{ _commits(docID: "{doc_id}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#
        );
        let read_commits = |identity: identity::Did| {
            query::QueryRequest::new(&query).with_identity(Some(identity))
        };
        let owner_before = node.execute_request(read_commits(owner.clone())).await;
        assert!(!owner_before.has_errors(), "{:?}", owner_before.errors);
        let cid = owner_before.data.unwrap()["_commits"][0]["cid"]
            .as_str()
            .unwrap()
            .to_owned();
        let foreign_before = node.execute_request(read_commits(foreign.clone())).await;
        assert!(!foreign_before.has_errors(), "{:?}", foreign_before.errors);
        assert!(foreign_before.data.unwrap()["_commits"].as_array().unwrap().is_empty());
        assert!(node.authorized_signed_block_bytes(&cid, Some(DID_A)).await.is_ok());
        assert!(node.authorized_signed_block_bytes(&cid, Some(DID_B)).await.is_err());

        let deleted = node
            .execute_request(
                query::QueryRequest::new(format!(
                    "mutation {{ delete_Users(docID: \"{doc_id}\") {{ _docID }} }}"
                ))
                .with_identity(Some(owner)),
            )
            .await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        let foreign_after = node.execute_request(read_commits(foreign)).await;
        assert!(!foreign_after.has_errors(), "{:?}", foreign_after.errors);
        let visible_commits = foreign_after.data.unwrap()["_commits"]
            .as_array()
            .unwrap()
            .len();
        let visible_signed_block = node.authorized_signed_block_bytes(&cid, Some(DID_B)).await.is_ok();
        node.shutdown().await.unwrap();
        assert!(
            visible_commits == 0 && !visible_signed_block,
            "foreign actor sees {visible_commits} historical commits; signed block readable: {visible_signed_block}"
        );
    })
    .unwrap()
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn deleted_history_access_and_revocation_match_the_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let observed = workers::block_on(async {
        let node = Node::builder().enable_signing().build().await.unwrap();
        let owner = identity::Did::new(DID_A).unwrap();
        let reader = identity::Did::new(DID_B).unwrap();
        let foreign = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap())
            .unwrap().did().unwrap();
        let policy = node.add_dac_policy_as(
            &defra_core::RequestContext::default().with_identity(Some(DID_A.to_owned())),
            POLICY_YAML,
        ).await.unwrap();
        node.add_collection(&format!("type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}")).await.unwrap();
        let create = node.execute_request(query::QueryRequest::new(
            "mutation { add_Users(input: {name: \"secret\"}) { _docID } }"
        ).with_identity(Some(owner.clone()))).await;
        assert!(!create.has_errors(), "{:?}", create.errors);
        let doc = create.data.unwrap()["add_Users"][0]["_docID"].as_str().unwrap().to_owned();
        node.add_dac_actor_relationship(DID_A, "Users", &doc, "reader", DID_B).await.unwrap();
        let query = format!(r#"query {{ _commits(docID: "{doc}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#);
        let owner_before = node.execute_request(query::QueryRequest::new(&query).with_identity(Some(owner.clone()))).await;
        assert!(!owner_before.has_errors(), "{:?}", owner_before.errors);
        let cid = owner_before.data.unwrap()["_commits"][0]["cid"].as_str().unwrap().to_owned();
        let deleted = node.execute_request(query::QueryRequest::new(format!(
            "mutation {{ delete_Users(docID: \"{doc}\") {{ _docID }} }}"
        )).with_identity(Some(owner.clone()))).await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        let mut actual = Vec::new();
        for actor in [&owner, &reader, &foreign] {
            let result = node.execute_request(query::QueryRequest::new(&query).with_identity(Some(actor.clone()))).await;
            assert!(!result.has_errors(), "{:?}", result.errors);
            let commits = !result.data.unwrap()["_commits"].as_array().unwrap().is_empty();
            let block = node.authorized_signed_block_bytes(&cid, Some(actor.as_str())).await.is_ok();
            actual.push((commits, block));
        }
        let revoked = node.delete_dac_actor_relationship(DID_A, "Users", &doc, "reader", DID_B).await.is_ok();
        let result = node.execute_request(query::QueryRequest::new(&query).with_identity(Some(reader.clone()))).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let commits = !result.data.unwrap()["_commits"].as_array().unwrap().is_empty();
        let block = node.authorized_signed_block_bytes(&cid, Some(reader.as_str())).await.is_ok();
        actual.push((commits, block));
        node.shutdown().await.unwrap();
        (actual, revoked)
    }).unwrap();

    let live =
        serde_json::json!({"live":true, "history":true, "registered":true, "readerGranted":true});
    let deleted =
        serde_json::json!({"live":false, "history":true, "registered":true, "readerGranted":true});
    let inputs = serde_json::json!([
        {"state":live, "actor":"owner", "action":"delete", "retainsHistory":true},
        {"state":live, "actor":"reader", "action":"delete", "retainsHistory":true},
        {"state":live, "actor":"foreign", "action":"delete", "retainsHistory":true},
        {"state":deleted, "actor":"reader", "action":"revoke", "retainsHistory":true}
    ]);
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "DeletedHistory.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{inputs}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let pairs = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let allowed = case["read"].as_bool().unwrap();
            (allowed, allowed)
        })
        .collect::<Vec<_>>();
    assert!(observed.1, "revocation of retained history must succeed");
    assert_eq!(observed.0, pairs);
}

#[test]
fn purged_document_must_not_reuse_retained_reader_grants() {
    workers::block_on(async {
        let (node, doc_id) = node_with_protected_users().await;
        let owner = identity::Did::new(DID_A).unwrap();
        let reader = identity::Did::new(DID_B).unwrap();
        let schema = node.get_collection("Users").unwrap().unwrap();
        let policy = schema.policy.as_ref().unwrap();
        node.add_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
            .await
            .unwrap();
        let deleted = node
            .execute_request(
                query::QueryRequest::new(format!(
                    "mutation {{ delete_Users(docID: \"{doc_id}\") {{ _docID }} }}"
                ))
                .with_identity(Some(owner.clone())),
            )
            .await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);

        // Model the candidate soft-delete fix while leaving production code unchanged.
        let acp = node.document_acp();
        acp.register_doc_object(&owner, &policy.id, &policy.resource_name, &doc_id)
            .await
            .unwrap();
        node.add_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
            .await
            .unwrap();
        let purged = node
            .database()
            .purge_by_doc_ids(
                "Users",
                &[document::DocID::from_string(&doc_id).unwrap()],
                Some(&owner),
            )
            .await
            .unwrap();
        assert_eq!(purged, 1);
        let recreated = node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Users(input: {name: \"secret\"}) { _docID } }",
                )
                .with_identity(Some(owner)),
            )
            .await;
        assert!(!recreated.has_errors(), "{:?}", recreated.errors);
        assert_eq!(recreated.data.unwrap()["add_Users"][0]["_docID"], doc_id);
        let visible = visible_users(&node, Some(reader)).await;
        node.shutdown().await.unwrap();
        assert_eq!(
            visible, 0,
            "a recreated document inherited a previous reader grant"
        );
    })
    .unwrap()
}

#[test]
fn add_dac_policy_rejects_empty_identity_and_policy() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let err = node
            .add_dac_policy_as(
                &defra_core::RequestContext::ambient().with_identity(Some(String::from(""))),
                POLICY_YAML,
            )
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "policy creator can not be empty");
        let err = node
            .add_dac_policy_as(
                &defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))),
                "",
            )
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "policy data can not be empty");
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn add_dac_policy_returns_id_and_validates_policy() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    let policy_id = node.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML).await.unwrap();
    assert!(!policy_id.is_empty());
    let bad = "name: Bad\nresources:\n  - name: users\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: undeclared\n";
    assert!(node.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), bad).await.is_err());
    node.shutdown().await.unwrap();
})
    .unwrap()
}

#[test]
fn add_collection_rejects_unregistered_policy_id() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let err = node
            .add_collection(
                "type Users @policy(id: \"deadbeef\", resource: \"users\") { name: String }",
            )
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("policyID specified does not exist with acp. PolicyID: deadbeef"),
            "{err:#}"
        );

        node.add_collection("type Plain { name: String }")
            .await
            .expect("a schema without @policy must not need the policy store");
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn add_view_rejects_unregistered_policy_id() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        node.add_collection("type Plain { name: String }")
            .await
            .unwrap();

        let err = node
            .add_view(
                "Plain { name }",
                "type PlainView @policy(id: \"deadbeef\", resource: \"users\") { name: String }",
            )
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("schema policy validation error"),
            "{err:#}"
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn dac_share_journey_grant_and_revoke() {
    workers::block_on(async {
        let (node, doc_id) = node_with_protected_users().await;
        let did_b = identity::Did::new(DID_B).unwrap();

        assert_eq!(visible_users(&node, Some(did_b.clone())).await, 0);
        assert_eq!(visible_users(&node, None).await, 0);

        let mut updates = node.subscribe(&[events::EventName::Update]);
        let existed = node
            .add_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
            .await
            .unwrap();
        assert!(!existed, "first grant must be newly added");
        let message = workers::timeout(std::time::Duration::from_secs(2), updates.recv())
            .await
            .expect("new grant must publish a doc-update event")
            .expect("event bus closed");
        let update = message.as_update().expect("document update");
        assert_eq!(update.doc_id, doc_id);
        assert!(!update.block.is_empty());
        assert_eq!(visible_users(&node, Some(did_b.clone())).await, 1);

        assert!(
            node.add_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
                .await
                .unwrap(),
            "second grant reports existed_already"
        );
        assert!(
            workers::timeout(std::time::Duration::from_millis(500), updates.recv())
                .await
                .is_err(),
            "an existed-already grant must not publish a doc-update event"
        );

        assert!(node
            .delete_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
            .await
            .unwrap());
        assert!(
            workers::timeout(std::time::Duration::from_millis(500), updates.recv())
                .await
                .is_err(),
            "a revoke must not publish a doc-update event"
        );
        assert_eq!(visible_users(&node, Some(did_b)).await, 0);
        assert!(!node
            .delete_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", DID_B)
            .await
            .unwrap());
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn dac_relationship_rejects_owner_relation() {
    workers::block_on(async {
        let (node, doc_id) = node_with_protected_users().await;

        let add = node
            .add_dac_actor_relationship(DID_A, "Users", &doc_id, "owner", DID_B)
            .await
            .unwrap_err();
        assert_eq!(
            add.to_string(),
            "OPERATION_FORBIDDEN: cannot add owner relation"
        );
        let delete = node
            .delete_dac_actor_relationship(DID_A, "Users", &doc_id, "owner", DID_B)
            .await
            .unwrap_err();
        assert_eq!(
            delete.to_string(),
            "OPERATION_FORBIDDEN: cannot delete owner relation"
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn dac_wildcard_grant_opens_document_to_every_actor() {
    workers::block_on(async {
        let (node, doc_id) = node_with_protected_users().await;

        assert!(!node
            .add_dac_actor_relationship(DID_A, "Users", &doc_id, "reader", "*")
            .await
            .unwrap());
        assert_eq!(visible_users(&node, None).await, 1);
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn dac_relationship_requires_a_collection_policy() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        node.add_collection("type Plain { name: String }")
            .await
            .unwrap();

        let add = node
            .add_dac_actor_relationship(DID_A, "Plain", "doc-1", "reader", DID_B)
            .await
            .unwrap_err();
        assert_eq!(
            add.to_string(),
            "operation requires ACP, but collection has no policy"
        );
        let delete = node
            .delete_dac_actor_relationship(DID_A, "Plain", "doc-1", "reader", DID_B)
            .await
            .unwrap_err();
        assert_eq!(
            delete.to_string(),
            "operation requires ACP, but collection has no policy"
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn document_acp_handle_registers_and_checks_a_document() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let policy_id = node
            .add_dac_policy_as(
                &defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))),
                POLICY_YAML,
            )
            .await
            .unwrap();
        let did_a = identity::Did::new(DID_A).unwrap();

        let document_acp = node.document_acp();
        document_acp
            .register_doc_object(&did_a, &policy_id, "users", "doc-x")
            .await
            .unwrap();

        assert!(document_acp
            .check_doc_access(
                &acp::Identity::Authenticated(did_a),
                acp::DocumentPermission::Read,
                &policy_id,
                "users",
                "doc-x",
            )
            .await
            .unwrap());
        assert!(!document_acp
            .check_doc_access(
                &acp::Identity::Anonymous,
                acp::DocumentPermission::Read,
                &policy_id,
                "users",
                "doc-x",
            )
            .await
            .unwrap());
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn node_identity_does_not_bypass_protected_history_reads() {
    workers::block_on(async {
    let _serial = SIGNING_STORE_GUARD.lock().await;
    defra_core::signing::clear_identity_store();
    let node_did = register_local_node_identity();
    let node_identity = identity::Did::new(&node_did).unwrap();
    let owner = identity::Did::new(DID_A).unwrap();

    let node = Node::builder()
        .with_node_identity_did(&node_did)
        .build()
        .await
        .unwrap();
    let policy_id = node.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML).await.unwrap();
    node.add_collection(&format!(
        "type Users @policy(id: \"{policy_id}\", resource: \"users\") {{ name: String }}"
    ))
    .await
    .unwrap();

    let create = node
        .execute_request(
            query::QueryRequest::new(
                "mutation { add_Users(input: {name: \"secret\"}) { _docID } }",
            )
            .with_identity(Some(owner.clone())),
        )
        .await;
    assert!(!create.has_errors(), "{:?}", create.errors);
    let doc_id = create.data.as_ref().unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap();

    assert_eq!(visible_users(&node, Some(node_identity.clone())).await, 0);

    let commits_query = format!(
        r#"query {{ _commits(docID: "{doc_id}", filter: {{fieldName: {{_eq: "_C"}}}}) {{ cid }} }}"#
    );
    let node_commits = node
        .execute_request(
            query::QueryRequest::new(&commits_query).with_identity(Some(node_identity.clone())),
        )
        .await;
    assert!(!node_commits.has_errors(), "{:?}", node_commits.errors);
    assert!(node_commits.data.unwrap()["_commits"]
        .as_array()
        .unwrap()
        .is_empty());

    let owner_commits = node
        .execute_request(query::QueryRequest::new(&commits_query).with_identity(Some(owner)))
        .await;
    assert!(!owner_commits.has_errors(), "{:?}", owner_commits.errors);
    let cid = owner_commits.data.unwrap()["_commits"][0]["cid"]
        .as_str()
        .unwrap()
        .to_string();

    node.authorized_signed_block_bytes(&cid, Some(&node_did))
        .await
        .expect_err("the process owner must satisfy document ACP when reading signed blocks");
    node.authorized_signed_block_bytes(&cid, Some(DID_A))
        .await
        .expect("the document owner may read signed block material");

    node.shutdown().await.unwrap();
    defra_core::signing::clear_identity_store();
})
    .unwrap()
}

#[test]
fn add_dac_policy_is_gated_by_node_access_control() {
    workers::block_on(async {
        let _serial = SIGNING_STORE_GUARD.lock().await;
        defra_core::signing::clear_identity_store();
        let node_did = register_local_node_identity();

        let node = Node::builder()
            .with_node_identity_did(&node_did)
            .with_node_acp_enabled()
            .build()
            .await
            .unwrap();

        let err = node
            .add_dac_policy_as(
                &defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_B))),
                POLICY_YAML,
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "not authorized to perform operation. Permission: add-dac-policy"
        );
        node.add_dac_policy_as(
            &defra_core::RequestContext::ambient().with_identity(Some(String::from(&node_did))),
            POLICY_YAML,
        )
        .await
        .expect("node identity must bypass NAC");

        node.shutdown().await.unwrap();
        defra_core::signing::clear_identity_store();
    })
    .unwrap()
}

/// Local ACP policy ids are salted with a per-process counter, so a peer
/// whose node has already registered other policies cannot reproduce a
/// creator's policy id through add_dac_policy — and a policy-bound SDL
/// (which embeds the id) would then derive a different collection id. The
/// explicit-counter variant makes the id reproducible and idempotent.
#[test]
fn policy_id_reproducible_with_explicit_counter() {
    workers::block_on(async {
    let creator = Node::builder().build().await.unwrap();
    let follower = Node::builder().build().await.unwrap();
    let id = creator.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML).await.unwrap();

    let unrelated = "name: unrelated\nresources:\n  - name: users\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n";
    follower.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), unrelated).await.unwrap();
    let diverged = follower.add_dac_policy_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML).await.unwrap();
    assert_ne!(
        diverged, id,
        "plain re-registration diverges after unrelated history"
    );

    let reproduced = follower
        .add_dac_policy_with_counter_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML, 1)
        .await
        .unwrap();
    assert_eq!(reproduced, id);
    let again = follower
        .add_dac_policy_with_counter_as(&defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))), POLICY_YAML, 1)
        .await
        .unwrap();
    assert_eq!(again, id);

    follower
        .add_collection(&format!(
            "type Users @policy(id: \"{reproduced}\", resource: \"users\") {{ name: String }}"
        ))
        .await
        .unwrap();
    creator.shutdown().await.unwrap();
    follower.shutdown().await.unwrap();
})
    .unwrap()
}

/// N identical policies added at once through the embedded API get N distinct
/// IDs, and IDs stay distinct after the node restarts over the same store.
#[test]
fn concurrent_identical_policy_adds_get_distinct_ids_across_a_restart() {
    workers::block_on(async {
        const ADDS: usize = 8;
        let dir = tempfile::tempdir().unwrap();
        let mut ids = Vec::new();
        {
            let node =
                std::sync::Arc::new(Node::builder().data_path(dir.path()).build().await.unwrap());
            let adds: Vec<_> = (0..ADDS)
                .map(|_| {
                    let node = std::sync::Arc::clone(&node);
                    workers::spawn(None, "test", move || async move {
                        node.add_dac_policy_as(
                            &defra_core::RequestContext::ambient()
                                .with_identity(Some(String::from(DID_A))),
                            POLICY_YAML,
                        )
                        .await
                    })
                    .unwrap()
                })
                .collect();
            for add in adds {
                ids.push(add.await.unwrap().unwrap());
            }
            node.shutdown().await.unwrap();
        }
        let node = Node::builder().data_path(dir.path()).build().await.unwrap();
        ids.push(
            node.add_dac_policy_as(
                &defra_core::RequestContext::ambient().with_identity(Some(String::from(DID_A))),
                POLICY_YAML,
            )
            .await
            .unwrap(),
        );
        node.shutdown().await.unwrap();

        let mut distinct = ids.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), ADDS + 1, "repeated policy IDs: {ids:?}");
    })
    .unwrap()
}
