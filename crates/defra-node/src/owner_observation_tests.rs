use super::Node;
use defra_core::RequestContext;
use identity::Did;

const A: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const B: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";

async fn fixture() -> (Node, String, String) {
    let node = Node::builder().build().await.unwrap();
    let policy = node
        .add_dac_policy_as(
            &defra_core::RequestContext::ambient().with_identity(Some(String::from(A))),
            POLICY,
        )
        .await
        .unwrap();
    node.add_collection(&format!("type Users @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}\ntype Other {{ name: String }}"))
        .await.unwrap();
    let response = node
        .execute_request(
            query::QueryRequest::new(
                "mutation { add_Users(input: {name: \"private\"}) { _docID } }",
            )
            .with_identity(Some(Did::new(A).unwrap())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let doc = response.data.unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    (node, policy, doc)
}

fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

#[test]
fn owner_observation_preserves_actor_document_and_collection_boundaries() {
    workers::block_on(async {
        let (node, _, doc) = fixture().await;
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let result = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(A).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(result[0].doc_id, doc);
        assert!(result[0].visible);
        assert_eq!(result[0].owner.as_ref().map(Did::as_str), Some(A));
        let denied = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(B).unwrap()),
            )
            .await
            .unwrap();
        assert!(denied[0].owner.is_none());
        assert!(!denied[0].visible);
        assert!(node
            .runner()
            .observe_document_owners_in_txn(
                actor(B),
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(A).unwrap()),
            )
            .await
            .is_err());
        let wrong = node
            .observe_document_owners_in_txn(
                &handle,
                "Other",
                std::slice::from_ref(&doc),
                Some(Did::new(A).unwrap()),
            )
            .await;
        let observations = wrong.unwrap();
        assert_eq!(observations.len(), 1);
        assert!(observations[0].owner.is_none());
        assert!(!observations[0].visible);
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        let hidden = node
            .runner()
            .begin_txn_as(&RequestContext::default(), true)
            .await
            .unwrap();
        let result = node
            .observe_document_owners_in_txn(&hidden, "Users", &[doc], Some(Did::new(B).unwrap()))
            .await
            .unwrap();
        assert!(result[0].owner.is_none());
        assert!(!result[0].visible);
        node.runner()
            .discard_txn_as(&RequestContext::default(), &hidden)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn owner_observation_keeps_the_row_snapshot_after_external_registration_changes() {
    workers::block_on(async {
        let (node, policy, doc) = fixture().await;
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let before = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(A).unwrap()),
            )
            .await
            .unwrap();
        let acp = node.document_acp();
        let moved = doc.clone();
        node.on_acp(move || async move {
            acp.unregister_doc_object(&policy, "rows", &moved).await?;
            acp.register_doc_object(&Did::new(B).unwrap(), &policy, "rows", &moved)
                .await?;
            Ok(())
        })
        .await
        .unwrap();
        let after = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(A).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(before, after);
        assert!(after[0].visible);
        assert_eq!(after[0].owner.as_ref().map(Did::as_str), Some(A));
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        let next = node
            .runner()
            .begin_txn_as(&RequestContext::default(), true)
            .await
            .unwrap();
        let current = node
            .observe_document_owners_in_txn(&next, "Users", &[doc], Some(Did::new(B).unwrap()))
            .await
            .unwrap();
        assert_eq!(current[0].owner.as_ref().map(Did::as_str), Some(B));
        assert!(current[0].visible);
        let denied = node
            .observe_document_owners_in_txn(
                &next,
                "Users",
                std::slice::from_ref(&current[0].doc_id),
                Some(Did::new(A).unwrap()),
            )
            .await
            .unwrap();
        assert!(!denied[0].visible);
        assert!(denied[0].owner.is_none());
        node.runner()
            .discard_txn_as(&RequestContext::default(), &next)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn owner_observation_bounds_requests_and_keeps_unregistered_ownership_unknown() {
    workers::block_on(async {
        let (node, policy, doc) = fixture().await;
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        for (collection, ids) in [
            ("Users", Vec::new()),
            (
                "Users",
                vec![doc.clone(); query::owners::MAX_OWNER_OBSERVATIONS + 1],
            ),
            ("Users) { Other", vec![doc.clone()]),
        ] {
            assert!(node
                .observe_document_owners_in_txn(
                    &handle,
                    collection,
                    &ids,
                    Some(Did::new(A).unwrap())
                )
                .await
                .is_err());
        }
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        let acp = node.document_acp();
        let cleared = doc.clone();
        node.on_acp(move || async move {
            acp.unregister_doc_object(&policy, "rows", &cleared).await?;
            Ok(())
        })
        .await
        .unwrap();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let result = node
            .observe_document_owners_in_txn(&handle, "Users", &[doc], Some(Did::new(A).unwrap()))
            .await
            .unwrap();
        assert!(result[0].owner.is_none());
        assert!(result[0].visible);
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn owner_observation_does_not_treat_a_deleted_document_as_a_live_member() {
    workers::block_on(async {
        let (node, policy, doc) = fixture().await;
        let response = node
            .execute_request(
                query::QueryRequest::new(format!(
                    "mutation {{ delete_Users(docID: \"{doc}\") {{ _docID }} }}",
                ))
                .with_identity(Some(Did::new(A).unwrap())),
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let acp = node.document_acp();
        let stale = doc.clone();
        node.on_acp(move || async move {
            acp.register_doc_object(&Did::new(A).unwrap(), &policy, "rows", &stale)
                .await?;
            Ok(())
        })
        .await
        .unwrap();
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let result = node
            .observe_document_owners_in_txn(&handle, "Users", &[doc], Some(Did::new(A).unwrap()))
            .await
            .unwrap();
        assert!(result[0].owner.is_none());
        assert!(!result[0].visible);
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn owner_observation_reports_the_registrant_to_a_different_authorized_reader() {
    workers::block_on(async {
        let (node, policy, doc) = fixture().await;
        node.document_acp()
            .add_actor_relationship(
                &Did::new(A).unwrap(),
                &Did::new(B).unwrap(),
                &policy,
                "rows",
                &doc,
                "reader",
                &[],
            )
            .await
            .unwrap();
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let observations = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                std::slice::from_ref(&doc),
                Some(Did::new(B).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert!(observations[0].visible);
        assert_eq!(observations[0].owner.as_ref().map(Did::as_str), Some(A));
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn owner_visibility_distinguishes_public_rows_from_denied_and_missing_rows() {
    workers::block_on(async {
        let (node, _, private) = fixture().await;
        let created = node
            .execute_request(
                query::QueryRequest::new(
                    "mutation { add_Other(input: {name: \"public\"}) { _docID } }",
                )
                .with_identity(Some(Did::new(A).unwrap())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let public = created.data.unwrap()["add_Other"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        let context = RequestContext::default();
        let handle = node.runner().begin_txn_as(&context, true).await.unwrap();
        let rows = node
            .observe_document_owners_in_txn(
                &handle,
                "Other",
                &[public.clone(), private.clone()],
                Some(Did::new(B).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].doc_id, public);
        assert!(rows[0].visible);
        assert!(rows[0].owner.is_none());
        assert_eq!(rows[1].doc_id, private);
        assert!(!rows[1].visible);
        assert!(rows[1].owner.is_none());
        let hidden = node
            .observe_document_owners_in_txn(
                &handle,
                "Users",
                &[private],
                Some(Did::new(B).unwrap()),
            )
            .await
            .unwrap();
        assert!(!hidden[0].visible);
        assert!(hidden[0].owner.is_none());
        node.runner()
            .discard_txn_as(&context, &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

fn modeled_access(mode: &str, facts: [bool; 13]) -> super::DocumentAccessDecision {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("lake")
        .current_dir(root.join("proofs/lean"))
        .args(["env", "lean", "--run", "AccessOracle.lean", mode])
        .args(facts.map(|value| if value { "1" } else { "0" }))
        .output()
        .expect("run Lean access owner");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    match String::from_utf8(output.stdout).unwrap().trim() {
        "allowed" => super::DocumentAccessDecision::Allowed,
        "denied" => super::DocumentAccessDecision::Denied,
        "unavailable" => super::DocumentAccessDecision::Unavailable,
        other => panic!("unexpected Lean access result: {other}"),
    }
}

#[test]
fn unified_access_matches_lean_and_validates_authority_at_commit() {
    use super::{DocumentAccessMode, NodePermission};
    use crypto::Key;
    use identity::Identity as _;
    if !std::process::Command::new("lake")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let hidden = modeled_access(
        "read",
        [
            true, true, true, true, true, true, false, true, true, true, true, false, false,
        ],
    );
    let visible = modeled_access(
        "read",
        [
            true, true, true, true, true, true, true, true, true, true, true, false, false,
        ],
    );
    let denied_node = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, true, true, true, false, true,
        ],
    );
    let denied_dac = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, true, true, true, true, false,
        ],
    );
    let allowed = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, true, true, true, true, true,
        ],
    );
    let missing = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, false, true, true, true, true, true,
        ],
    );
    let unregistered = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, false, true, true, true, true,
        ],
    );
    let inactive = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, true, true, false, true, true,
        ],
    );
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let key = crypto::generate_ed25519().unwrap();
        let identity = identity::RawIdentity::from_ed25519(key.clone()).unwrap();
        let owner = identity.did().unwrap().to_string();
        let public = identity.public_key_bytes();
        defra_core::signing::store_identity(&owner, defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(), public_key_hex: hex::encode(&public),
            public_key_bytes: public, remote_signer: None, signing_authorization: None,
        }).expect("the store takes the identity");
        let node = Node::builder().with_node_identity_did(&owner).with_node_acp_enabled().build().await.unwrap();
        let policy = node.add_dac_policy_as(&actor(&owner), POLICY).await.unwrap();
        node.add_collection(&format!("type Users @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}\ntype Other {{ name: String }}")).await.unwrap();
        let response = node.execute("mutation { add_Users(input: {name: \"private\"}) { _docID } }").await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc = response.data.unwrap()["add_Users"][0]["_docID"].as_str().unwrap().to_owned();
        let subject = Did::new(B).unwrap();
        let ids = vec![doc.clone(), "missing".into()];
        let named = || DocumentAccessMode::Named { permission: "read".into(), node_permission: NodePermission::DocumentRead };
        let txn = node.begin_transaction(false).await.unwrap();
        defra_core::dac_bypass::set_dac_bypass(true);
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await.unwrap(), [hidden, hidden]);
        defra_core::dac_bypass::set_dac_bypass(false);
        assert!(node.runner().observe_document_access_in_txn(actor(B), &txn, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await.is_err());
        node.discard_transaction(&txn).await.unwrap();
        let denied_context = actor(B);
        let denied_handle = node.runner().begin_txn_as(&denied_context, true).await.unwrap();
        let denied = node.runner().observe_document_access_in_txn(denied_context.clone(), &denied_handle, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await;
        assert!(denied.is_err(), "the transaction principal must pass the node read gate");
        node.runner().discard_txn_as(&denied_context, &denied_handle).await.unwrap();
        node.add_dac_actor_relationship(&owner, "Users", &doc, "reader", B).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await.unwrap(), [visible, hidden]);
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [denied_node, missing]);
        node.discard_transaction(&txn).await.unwrap();
        let nac = node.nac_manager().clone();
        let owner_did = Did::new(&owner).unwrap();
        let actor_did = subject.clone();
        node.on_acp(move || async move {
            nac.add_permission_grant(&owner_did, &actor_did, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?;
            nac.add_permission_grant(&owner_did, &actor_did, NodePermission::DocumentUpdate).await.map_err(crate::Error::NacInit)?;
            Ok(())
        }).await.unwrap();
        let principal_context = actor(B);
        let principal_txn = node.runner().begin_txn_as(&principal_context, false).await.unwrap();
        assert_eq!(node.runner().observe_document_access_in_txn(principal_context.clone(), &principal_txn, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await.unwrap(), [visible, hidden]);
        let principal_write = query::QueryRequest::new("mutation { add_Other(input: {name: \"guarded\"}) { _docID } }")
            .with_identity(Some(subject.clone())).with_context(principal_context.clone());
        let response = node.runner().execute_in_txn(principal_write, &principal_txn).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let nac = node.nac_manager().clone();
        let owner_did = Did::new(&owner).unwrap();
        let actor_did = subject.clone();
        node.on_acp(move || async move { nac.remove_permission_grant(&owner_did, &actor_did, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        assert!(node.runner().commit_txn_as(&principal_context, &principal_txn).await.is_err(), "principal NAC revocation must conflict with delegated read's guarded commit");
        let nac = node.nac_manager().clone();
        let owner_did = Did::new(&owner).unwrap();
        let actor_did = subject.clone();
        node.on_acp(move || async move { nac.add_permission_grant(&owner_did, &actor_did, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [allowed, missing]);
        let response = node.execute_request_in_txn(query::QueryRequest::new("mutation { add_Other(input: {name: \"guarded\"}) { _docID } }"), &txn).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let nac = node.nac_manager().clone();
        let owner_did = Did::new(&owner).unwrap();
        let actor_did = subject.clone();
        node.on_acp(move || async move { nac.remove_permission_grant(&owner_did, &actor_did, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err(), "actor NAC revocation must conflict with guarded commit");
        let nac = node.nac_manager().clone();
        let owner_did = Did::new(&owner).unwrap();
        let actor_did = subject.clone();
        node.on_acp(move || async move { nac.add_permission_grant(&owner_did, &actor_did, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [allowed, missing]);
        let response = node.execute_request_in_txn(query::QueryRequest::new("mutation { add_Other(input: {name: \"guarded\"}) { _docID } }"), &txn).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        node.delete_dac_actor_relationship(&owner, "Users", &doc, "reader", B).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err(), "named DAC revocation must conflict with guarded commit");
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [denied_dac, missing]);
        node.discard_transaction(&txn).await.unwrap();
        node.add_dac_actor_relationship(&owner, "Users", &doc, "reader", B).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [allowed, missing]);
        let response = node.execute_request_in_txn(query::QueryRequest::new("mutation { add_Other(input: {name: \"guarded\"}) { _docID } }"), &txn).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let nac = node.nac_manager().clone();
        let caller = db::NacCaller { identity: Some(Did::new(&owner).unwrap()), node: Some(Did::new(&owner).unwrap()) };
        node.on_acp(move || async move { nac.disable_as(&caller).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        assert!(node.commit_transaction(&txn).await.is_err(), "disabling NAC must conflict with guarded commit");
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [inactive, missing]);
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), DocumentAccessMode::DelegatedRead).await.unwrap(), [visible, hidden]);
        node.discard_transaction(&txn).await.unwrap();
        let nac = node.nac_manager().clone();
        let caller = db::NacCaller { identity: Some(Did::new(&owner).unwrap()), node: Some(Did::new(&owner).unwrap()) };
        node.on_acp(move || async move { nac.re_enable_as(&caller).await.map_err(crate::Error::NacInit)?; Ok(()) }).await.unwrap();
        let acp = node.document_acp();
        let policy_to_clear = policy.clone();
        let id_to_clear = doc.clone();
        node.on_acp(move || async move { acp.unregister_doc_object(&policy_to_clear, "rows", &id_to_clear).await?; Ok(()) }).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject.clone(), named()).await.unwrap(), [unregistered, missing]);
        node.discard_transaction(&txn).await.unwrap();
        let acp = node.document_acp();
        let policy_to_restore = policy.clone();
        let id_to_restore = doc.clone();
        let owner_to_restore = owner.clone();
        node.on_acp(move || async move { acp.register_doc_object(&Did::new(&owner_to_restore).unwrap(), &policy_to_restore, "rows", &id_to_restore).await?; Ok(()) }).await.unwrap();
        let response = node.execute_request(query::QueryRequest::new(format!("mutation {{ delete_Users(docID: \"{doc}\") {{ _docID }} }}"))).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let acp = node.document_acp();
        let id_to_restore = doc.clone();
        let owner_to_restore = owner.clone();
        node.on_acp(move || async move { acp.register_doc_object(&Did::new(&owner_to_restore).unwrap(), &policy, "rows", &id_to_restore).await?; Ok(()) }).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(node.observe_document_access_in_txn(&txn, "Users", &ids, subject, named()).await.unwrap(), [missing, missing]);
        node.discard_transaction(&txn).await.unwrap();
        node.shutdown().await.unwrap();
        defra_core::signing::clear_identity_store();
    }).unwrap();
}

#[test]
fn unified_named_access_follows_parent_and_conflicts_on_parent_revoke() {
    use super::{DocumentAccessMode, NodePermission};
    if !std::process::Command::new("lake")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let allowed = modeled_access(
        "named",
        [
            true, true, true, true, false, false, false, true, true, true, true, true, true,
        ],
    );
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let policy = node.add_dac_policy_as(&actor(A), r#"
name: filesystem
resources:
- name: directory
  relations:
  - name: reader
    types: [actor]
  permissions:
  - name: read
    expr: reader
  - name: update
  - name: delete
- name: file
  relations:
  - name: parent
    types: [directory]
  permissions:
  - name: read
    expr: parent->read
  - name: update
  - name: delete
"#).await.unwrap();
        node.add_collection(&format!("type Directory @policy(id: \"{policy}\", resource: \"directory\") {{ name: String }}\ntype File @policy(id: \"{policy}\", resource: \"file\") {{ name: String }}\ntype Other {{ name: String }}")).await.unwrap();
        let mut ids = Vec::new();
        for collection in ["Directory", "File"] {
            let response = node.execute_request(query::QueryRequest::new(format!("mutation {{ add_{collection}(input: {{name: \"x\"}}) {{ _docID }} }}")).with_identity(Some(Did::new(A).unwrap()))).await;
            assert!(!response.has_errors(), "{:?}", response.errors);
            ids.push(response.data.unwrap()[format!("add_{collection}")][0]["_docID"].as_str().unwrap().to_owned());
        }
        node.add_dac_actor_relationship(A, "Directory", &ids[0], "reader", B).await.unwrap();
        node.add_dac_actor_relationship(A, "File", &ids[1], "parent", &format!("directory:{}", ids[0])).await.unwrap();
        let nac = node.nac_manager().clone();
        node.on_acp(move || async move {
            let owner = Did::new(A).unwrap();
            let subject = Did::new(B).unwrap();
            nac.enable(&owner).await.map_err(crate::Error::NacInit)?;
            nac.add_permission_grant(&owner, &subject, NodePermission::DocumentRead).await.map_err(crate::Error::NacInit)?;
            Ok(())
        }).await.unwrap();
        let txn = node.begin_transaction(false).await.unwrap();
        let mode = DocumentAccessMode::Named { permission: "read".into(), node_permission: NodePermission::DocumentRead };
        assert_eq!(node.observe_document_access_in_txn(&txn, "File", &[ids[1].clone()], Did::new(B).unwrap(), mode).await.unwrap(), [allowed]);
        let response = node.execute_request_in_txn(query::QueryRequest::new("mutation { add_Other(input: {name: \"guarded\"}) { _docID } }").with_identity(Some(Did::new(A).unwrap())), &txn).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert!(node.delete_dac_actor_relationship(A, "Directory", &ids[0], "reader", B).await.unwrap());
        assert!(node.commit_transaction(&txn).await.is_err(), "revoking an inherited grant must conflict with the guarded commit");
        node.shutdown().await.unwrap();
    }).unwrap();
}
