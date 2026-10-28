use super::EmbeddedNode;
use defra_core::RequestContext;
use identity::Did;

const A: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const B: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";

async fn fixture() -> (EmbeddedNode, String, String) {
    let node = EmbeddedNode::builder().build().await.unwrap();
    let policy = node.add_dac_policy(A, POLICY).await.unwrap();
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
fn node_transaction_visibility_uses_actor_dac_and_node_nac() {
    workers::block_on(async {
        use crypto::Key;
        use identity::Identity as _;
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let key = crypto::generate_ed25519().unwrap();
        let identity = identity::RawIdentity::from_ed25519(key.clone()).unwrap();
        let owner = identity.did().unwrap().to_string();
        let public = identity.public_key_bytes();
        defra_core::signing::store_identity(&owner, defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(), public_key_hex: hex::encode(&public),
            public_key_bytes: public, remote_signer: None, signing_authorization: None,
        });
        let node = EmbeddedNode::builder().with_node_identity_did(&owner).with_node_acp_enabled().build().await.unwrap();
        let policy = node.add_dac_policy(&owner, POLICY).await.unwrap();
        node.add_collection(&format!("type Users @branchable @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}")).await.unwrap();
        let response = node.execute("mutation { add_Users(input: {name: \"private\"}) { _docID } }").await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc = response.data.unwrap()["add_Users"][0]["_docID"].as_str().unwrap().to_owned();
        let node_context = actor(&owner);
        let handle = node.runner().begin_txn_as(&node_context, true).await.unwrap();
        let denied = node.observe_document_visibility_in_txn(&handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.unwrap();
        assert!(!denied[0].visible);
        assert!(node.runner().observe_document_visibility_in_txn(actor(B), &handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.is_err());
        node.runner().discard_txn_as(&node_context, &handle).await.unwrap();
        node.add_dac_actor_relationship(&owner, "Users", &doc, "reader", B).await.unwrap();
        let handle = node.runner().begin_txn_as(&node_context, true).await.unwrap();
        let allowed = node.observe_document_visibility_in_txn(&handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.unwrap();
        assert!(allowed[0].visible);
        let missing = node.observe_document_visibility_in_txn(&handle, "Users", &["missing".into()], Did::new(B).unwrap()).await.unwrap();
        assert!(!missing[0].visible);
        node.runner().discard_txn_as(&node_context, &handle).await.unwrap();
        node.shutdown().await;
    }).unwrap();
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
        node.shutdown().await;
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
        node.shutdown().await;
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
        node.shutdown().await;
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
        node.shutdown().await;
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
        node.shutdown().await;
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
        node.shutdown().await;
    })
    .unwrap();
}
