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
        defra_core::dac_bypass::set_dac_bypass(true);
        let denied = node.observe_document_visibility_in_txn(&handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.unwrap();
        defra_core::dac_bypass::set_dac_bypass(false);
        assert!(!denied[0].visible);
        assert!(node.runner().observe_document_visibility_in_txn(actor(B), &handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.is_err());
        node.runner().discard_txn_as(&node_context, &handle).await.unwrap();
        node.add_dac_actor_relationship(&owner, "Users", &doc, "reader", B).await.unwrap();
        let handle = node.runner().begin_txn_as(&node_context, true).await.unwrap();
        let allowed = node.observe_document_visibility_in_txn(&handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.unwrap();
        assert!(allowed[0].visible);
        assert!(allowed[0].owner.is_none(), "delegated visibility exports no owner metadata");
        let missing = node.observe_document_visibility_in_txn(&handle, "Users", &["missing".into()], Did::new(B).unwrap()).await.unwrap();
        assert!(!missing[0].visible);
        node.runner().discard_txn_as(&node_context, &handle).await.unwrap();
        let denied_context = actor(B);
        let denied_handle = node.runner().begin_txn_as(&denied_context, true).await.unwrap();
        let error = node.runner().observe_document_visibility_in_txn(denied_context.clone(), &denied_handle, "Users", std::slice::from_ref(&doc), Did::new(B).unwrap()).await.unwrap_err();
        assert!(error.to_string().contains("transaction principal is not authorized"), "{error}");
        node.runner().discard_txn_as(&denied_context, &denied_handle).await.unwrap();
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

#[test]
fn session_and_working_agent_relationship_commit_and_rollback_together() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        let policy = node.add_dac_policy(A, POLICY).await.unwrap();
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}"
        ))
        .await
        .unwrap();
        let executor = node.runner();
        let owner = actor(A);
        let create = |name: &str| {
            query::QueryRequest::new(format!(
                "mutation {{ add_Users(input: {{ name: \"{name}\" }}) {{ _docID }} }}"
            ))
            .with_context(owner.clone())
            .with_identity(Some(Did::new(A).unwrap()))
        };
        let handle = executor.begin_txn_as(&owner, false).await.unwrap();
        let response = executor.execute_in_txn(create("discarded"), &handle).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc = response.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        executor
            .add_actor_relationship_in_txn(
                owner.clone(),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "reader",
                Did::new(B).unwrap(),
            )
            .await
            .unwrap();
        executor.discard_txn_as(&owner, &handle).await.unwrap();
        let after = node
            .execute_request(
                query::QueryRequest::new("{ Users { _docID } }")
                    .with_identity(Some(Did::new(A).unwrap())),
            )
            .await;
        assert!(after.data.unwrap()["Users"].as_array().unwrap().is_empty());

        let handle = executor.begin_txn_as(&owner, false).await.unwrap();
        let response = executor.execute_in_txn(create("committed"), &handle).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc = response.data.unwrap()["add_Users"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(executor
            .add_actor_relationship_in_txn(
                actor(B),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "reader",
                Did::new(B).unwrap()
            )
            .await
            .is_err());
        assert!(executor
            .add_actor_relationship_in_txn(
                owner.clone(),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "undeclared",
                Did::new(B).unwrap()
            )
            .await
            .is_err());
        executor
            .add_actor_relationship_in_txn(
                owner.clone(),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "reader",
                Did::new(B).unwrap(),
            )
            .await
            .unwrap();
        executor.commit_txn_as(&owner, &handle).await.unwrap();
        let visible = node
            .execute_request(
                query::QueryRequest::new("{ Users { _docID name } }")
                    .with_identity(Some(Did::new(B).unwrap())),
            )
            .await;
        assert_eq!(visible.data.unwrap()["Users"][0]["_docID"], doc);
        node.shutdown().await;
    })
    .unwrap();
}

#[test]
fn transactional_relationship_notifies_only_after_commit() {
    workers::block_on(async {
        let (node, _, doc) = fixture().await;
        let mut updates = node.subscribe(&[events::EventName::Update]);
        let executor = node.runner();
        let owner = actor(A);
        let handle = executor.begin_txn_as(&owner, false).await.unwrap();
        executor
            .add_actor_relationship_in_txn(
                owner.clone(),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "reader",
                Did::new(B).unwrap(),
            )
            .await
            .unwrap();
        assert!(
            workers::timeout(std::time::Duration::from_millis(100), updates.recv())
                .await
                .is_err()
        );
        executor.discard_txn_as(&owner, &handle).await.unwrap();
        assert!(
            workers::timeout(std::time::Duration::from_millis(100), updates.recv())
                .await
                .is_err()
        );

        let handle = executor.begin_txn_as(&owner, false).await.unwrap();
        executor
            .add_actor_relationship_in_txn(
                owner.clone(),
                &handle,
                Did::new(A).unwrap(),
                "Users",
                &doc,
                "reader",
                Did::new(B).unwrap(),
            )
            .await
            .unwrap();
        executor.commit_txn_as(&owner, &handle).await.unwrap();
        let message = workers::timeout(std::time::Duration::from_secs(2), updates.recv())
            .await
            .unwrap()
            .unwrap();
        let update = message.as_update().unwrap();
        assert_eq!(update.doc_id, doc);
        assert!(!update.block.is_empty());
        node.shutdown().await;
    })
    .unwrap();
}

#[test]
fn transactional_resource_links_propagate_parent_authority() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        let policy = node.add_dac_policy(A, "name: Parent links\nresources:\n  - name: nodes\n    permissions:\n      - name: read\n      - name: update\n      - name: delete\n  - name: agents\n    relations:\n      - name: parent\n        types: [nodes]\n    permissions:\n      - name: read\n        expr: parent->read\n      - name: update\n        expr: parent->update\n      - name: delete\n        expr: parent->delete\n  - name: sessions\n    relations:\n      - name: parent\n        types: [agents]\n    permissions:\n      - name: read\n        expr: parent->read\n      - name: update\n        expr: parent->update\n      - name: delete\n        expr: parent->delete\n").await.unwrap();
        for (collection, resource) in [("Nodes", "nodes"), ("Agents", "agents"), ("Sessions", "sessions")] {
            node.add_collection(&format!("type {collection} @policy(id: \"{policy}\", resource: \"{resource}\") {{ name: String }}")).await.unwrap();
        }
        let owner = node.execute_request(query::QueryRequest::new("mutation { add_Nodes(input: {name: \"home\"}) { _docID } }").with_identity(Some(Did::new(A).unwrap()))).await;
        assert!(!owner.has_errors(), "{:?}", owner.errors);
        let node_doc = owner.data.unwrap()["add_Nodes"][0]["_docID"].as_str().unwrap().to_owned();
        let executor = node.runner();
        let agent = actor(B);
        let txn = executor.begin_txn_as(&agent, false).await.unwrap();
        let create = |collection: &str, name: &str| query::QueryRequest::new(format!("mutation {{ add_{collection}(input: {{name: \"{name}\"}}) {{ _docID }} }}")).with_context(agent.clone()).with_identity(Some(Did::new(B).unwrap()));
        let created = executor.execute_in_txn(create("Agents", "agent"), &txn).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let agent_doc = created.data.unwrap()["add_Agents"][0]["_docID"].as_str().unwrap().to_owned();
        executor.add_relationship_in_txn(agent.clone(), &txn, Did::new(B).unwrap(), "Agents", &agent_doc, "parent", acp::Subject::entity_set("nodes", &node_doc, "")).await.unwrap();
        let created = executor.execute_in_txn(create("Sessions", "session"), &txn).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let session_doc = created.data.unwrap()["add_Sessions"][0]["_docID"].as_str().unwrap().to_owned();
        assert!(executor.add_relationship_in_txn(agent.clone(), &txn, Did::new(B).unwrap(), "Sessions", &session_doc, "parent", acp::Subject::entity_set("agents", "missing", "")).await.is_err());
        executor.add_relationship_in_txn(agent.clone(), &txn, Did::new(B).unwrap(), "Sessions", &session_doc, "parent", acp::Subject::entity_set("agents", &agent_doc, "")).await.unwrap();
        executor.commit_txn_as(&agent, &txn).await.unwrap();
        let visible = node.execute_request(query::QueryRequest::new("{ Sessions { _docID } }").with_identity(Some(Did::new(A).unwrap()))).await;
        assert_eq!(visible.data.unwrap()["Sessions"][0]["_docID"], session_doc);
        let deleted = node.execute_request(query::QueryRequest::new(format!("mutation {{ delete_Sessions(docID: \"{session_doc}\") {{ _docID }} }}")).with_identity(Some(Did::new(A).unwrap()))).await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        assert_eq!(deleted.data.unwrap()["delete_Sessions"][0]["_docID"], session_doc);
        node.shutdown().await;
    }).unwrap();
}

#[cfg(feature = "http")]
#[test]
fn http_relationship_uses_authenticated_transaction_owner() {
    use identity::Identity as _;
    use std::time::Duration;

    edge::testing::edge().block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = EmbeddedNode::builder()
            .with_http(crate::HttpConfig::with_listener(listener).unwrap())
            .build().await.unwrap();
        let policy = node.add_dac_policy(A, POLICY).await.unwrap();
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}"
        )).await.unwrap();
        let user = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let reader = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let token = |who: &identity::RawIdentity| {
            String::from_utf8(identity::new_token(who, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap()
        };
        let user_token = token(&user);
        let reader_token = token(&reader);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let base = format!("http://{address}/api/v0");
        let txn: serde_json::Value = client.post(format!("{base}/tx"))
            .bearer_auth(&user_token).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let txn = txn["id"].as_u64().unwrap().to_string();
        let created: serde_json::Value = client.post(format!("{base}/graphql"))
            .bearer_auth(&user_token).header("x-defradb-tx", &txn)
            .json(&serde_json::json!({"query":"mutation { add_Users(input: {name: \"http-user\"}) { _docID } }"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert!(created.get("errors").is_none(), "{created}");
        let doc = created["data"]["add_Users"][0]["_docID"].as_str().unwrap();
        let grant = serde_json::json!({"collection":"Users", "doc_id":doc, "relation":"reader", "target_actor":reader.did().unwrap().to_string()});
        let denied = client.post(format!("{base}/acp/document/relationship"))
            .bearer_auth(&reader_token).header("x-defradb-tx", &txn).json(&grant).send().await.unwrap();
        assert!(!denied.status().is_success());
        let added = client.post(format!("{base}/acp/document/relationship"))
            .bearer_auth(&user_token).header("x-defradb-tx", &txn).json(&grant).send().await.unwrap();
        assert!(added.status().is_success(), "{}", added.text().await.unwrap());
        client.post(format!("{base}/tx/{txn}")).bearer_auth(&user_token)
            .send().await.unwrap().error_for_status().unwrap();
        let visible: serde_json::Value = client.post(format!("{base}/graphql"))
            .bearer_auth(&reader_token).json(&serde_json::json!({"query":"{ Users { _docID name } }"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(visible["data"]["Users"][0]["_docID"], doc);
        node.shutdown().await;
    }).unwrap();
}

#[cfg(feature = "http")]
#[test]
fn enabled_node_acp_gates_http_and_allows_granted_document_access() {
    use crypto::Key as _;
    use identity::Identity as _;
    use std::time::Duration;

    edge::testing::edge().block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let key = crypto::generate_ed25519().unwrap();
        let runtime = identity::RawIdentity::from_ed25519(key.clone()).unwrap();
        let runtime_did = runtime.did().unwrap().to_string();
        let public = runtime.public_key_bytes();
        defra_core::signing::store_identity(&runtime_did, defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: key.raw().to_vec(), public_key_hex: hex::encode(&public),
            public_key_bytes: public, remote_signer: None, signing_authorization: None,
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = EmbeddedNode::builder().with_node_identity_did(&runtime_did)
            .with_node_acp_enabled().with_http(crate::HttpConfig::with_listener(listener).unwrap())
            .build().await.unwrap();
        let policy = node.add_dac_policy(&runtime_did, POLICY).await.unwrap();
        node.add_collection(&format!("type Users @policy(id: \"{policy}\", resource: \"rows\") {{ name: String }}")).await.unwrap();
        let user = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        assert!(node.add_collection_as("type UserSchemaDenied { name: String }", Some(user.did().unwrap())).await.is_err());
        node.add_collection("type RuntimeSchemaAllowed { name: String }").await.unwrap();
        let token = |who: &identity::RawIdentity| String::from_utf8(identity::new_token(who, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let base = format!("http://{address}/api/v0");
        let user_token = token(&user);
        let query = serde_json::json!({"query":"{ Users { _docID } }"});
        let denied = client.post(format!("{base}/graphql")).bearer_auth(&user_token).json(&query).send().await.unwrap();
        assert!(!denied.status().is_success());
        for relation in ["read-document", "update-document", "add-dac-relation"] {
            let response = client.post(format!("{base}/acp/node/relationship"))
                .bearer_auth(token(&runtime)).json(&serde_json::json!({"Relation":relation,"TargetActor":user.did().unwrap().to_string()}))
                .send().await.unwrap();
            assert!(response.status().is_success(), "{}", response.text().await.unwrap());
        }
        let allowed = client.post(format!("{base}/graphql")).bearer_auth(&user_token).json(&query).send().await.unwrap();
        assert!(allowed.status().is_success(), "{}", allowed.text().await.unwrap());
        node.shutdown().await;
    }).unwrap();
}

#[cfg(feature = "http")]
#[test]
fn http_resource_relationship_is_transaction_scoped() {
    use identity::Identity as _;
    use std::time::Duration;

    edge::testing::edge().block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = EmbeddedNode::builder().with_http(crate::HttpConfig::with_listener(listener).unwrap()).build().await.unwrap();
        let policy = node.add_dac_policy(A, "name: HTTP parent links\nresources:\n  - name: parents\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n  - name: children\n    relations:\n      - name: parent\n        types: [parents]\n    permissions:\n      - name: read\n        expr: parent->read\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection(&format!("type Parents @policy(id: \"{policy}\", resource: \"parents\") {{ name: String }}\ntype Children @policy(id: \"{policy}\", resource: \"children\") {{ name: String }}")).await.unwrap();
        let user = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let reader = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let token = |who: &identity::RawIdentity| String::from_utf8(identity::new_token(who, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let base = format!("http://{address}/api/v0");
        let txn: serde_json::Value = client.post(format!("{base}/tx")).bearer_auth(token(&user)).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let txn = txn["id"].as_u64().unwrap().to_string();
        let created: serde_json::Value = client.post(format!("{base}/graphql")).bearer_auth(token(&user)).header("x-defradb-tx", &txn)
            .json(&serde_json::json!({"query":"mutation { add_Parents(input:{name:\"p\"}) {_docID} add_Children(input:{name:\"c\"}) {_docID} }"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert!(created.get("errors").is_none(), "{created}");
        let parent = created["data"]["add_Parents"][0]["_docID"].as_str().unwrap();
        let child = created["data"]["add_Children"][0]["_docID"].as_str().unwrap();
        let actor_grant = serde_json::json!({"collection":"Parents", "doc_id":parent, "relation":"reader", "target_actor":reader.did().unwrap().to_string()});
        client.post(format!("{base}/acp/document/relationship")).bearer_auth(token(&user)).header("x-defradb-tx", &txn).json(&actor_grant).send().await.unwrap().error_for_status().unwrap();
        let link = serde_json::json!({"collection":"Children", "doc_id":child, "relation":"parent", "target_resource":"parents", "target_doc_id":parent});
        let denied = client.post(format!("{base}/acp/document/resource-relationship")).bearer_auth(token(&reader)).header("x-defradb-tx", &txn).json(&link).send().await.unwrap();
        assert!(!denied.status().is_success());
        client.post(format!("{base}/acp/document/resource-relationship")).bearer_auth(token(&user)).header("x-defradb-tx", &txn).json(&link).send().await.unwrap().error_for_status().unwrap();
        client.post(format!("{base}/tx/{txn}")).bearer_auth(token(&user)).send().await.unwrap().error_for_status().unwrap();
        let visible: serde_json::Value = client.post(format!("{base}/graphql")).bearer_auth(token(&reader))
            .json(&serde_json::json!({"query":"{ Children { _docID } }"})).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(visible["data"]["Children"][0]["_docID"], child);
        node.shutdown().await;
    }).unwrap();
}
