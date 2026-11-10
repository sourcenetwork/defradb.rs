use std::time::Duration;

use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Identity as _, RawIdentity};
use serde_json::{json, Value};

use crate::{HttpConfig, Node};

const POLICY: &str = r#"
name: HTTP Relationships
resources:
  - name: records
    relations:
      - name: reader
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;

fn identity() -> RawIdentity {
    RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap()
}

fn bearer(identity: &RawIdentity, audience: &str) -> String {
    let token = identity::new_token(
        identity,
        Duration::from_secs(60),
        Some(audience.into()),
        None,
    )
    .unwrap();
    format!("Bearer {}", String::from_utf8(token).unwrap())
}

async fn exercise_http_relationships() -> Vec<Value> {
    let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
    let owner = identity();
    let owner_did = owner.did().unwrap();
    let public = owner.public_key_bytes();
    defra_core::signing::store_identity(
        owner_did.as_str(),
        SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: owner.private_key_bytes(),
            public_key_bytes: public.clone(),
            public_key_hex: hex::encode(public),
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    let stranger = identity();
    let stranger_did = stranger.did().unwrap();
    let target = identity();
    let target_did = target.did().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let node = Node::builder()
        .with_node_identity_did(owner_did.as_str())
        .with_node_acp_enabled()
        .with_http(HttpConfig::with_listener(listener).unwrap())
        .build()
        .await
        .unwrap();
    let context = defra_core::RequestContext::default().with_identity(Some(owner_did.to_string()));
    let policy = node.add_dac_policy_as(&context, POLICY).await.unwrap();
    node.add_collection(&format!(
        "type Record @policy(id: \"{policy}\", resource: \"records\") {{ value: String }}"
    ))
    .await
    .unwrap();
    let created = node
        .execute_request(
            query::QueryRequest::new(
                r#"mutation { add_Record(input: {value: "secret"}) { _docID } }"#,
            )
            .with_identity(Some(owner_did.clone())),
        )
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    let doc = created.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    let native_created = node
        .execute_request(
            query::QueryRequest::new(
                r#"mutation { add_Record(input: {value: "native"}) { _docID } }"#,
            )
            .with_identity(Some(owner_did.clone())),
        )
        .await;
    assert!(!native_created.has_errors(), "{:?}", native_created.errors);
    let native_doc = native_created.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    for permission in [
        acp::nac::NodePermission::DocumentRead,
        acp::nac::NodePermission::CollectionGet,
    ] {
        node.add_nac_actor_relationship_as(&context, permission.as_str(), target_did.as_str())
            .await
            .unwrap();
    }
    let client = net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let owner_auth = bearer(&owner, &address.to_string());
    let node_grant = client
        .post(format!("http://{address}/api/v0/acp/node/relationship"))
        .header("Authorization", owner_auth.as_str())
        .json(&json!({
            "Relation": acp::nac::NodePermission::DacRelationAdd.as_str(),
            "TargetActor": stranger_did.as_str(),
        }))
        .send()
        .await
        .unwrap();
    assert!(
        node_grant.status().is_success(),
        "node grant failed: {}",
        node_grant.status()
    );
    let url = format!("http://{address}/api/v0/acp/document/relationship");
    let body = json!({"collection":"Record", "doc_id":doc, "relation":"reader", "target_actor":target_did.as_str()});
    let native_denied = node
        .add_dac_actor_relationship(
            stranger_did.as_str(),
            "Record",
            &native_doc,
            "reader",
            target_did.as_str(),
        )
        .await;
    assert!(native_denied.is_err());
    let denied = client
        .post(&url)
        .header("Authorization", bearer(&stranger, &address.to_string()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(
        !denied.status().is_success(),
        "a non-owner with NAC access must not grant the relationship"
    );
    let mut cases = vec![
        json!({"input":{"caller":stranger_did.as_str(), "target":target_did.as_str(), "nativeAllowed":native_denied.is_ok(), "nativeChanged":false}, "allowed":denied.status().is_success(), "changed":false}),
    ];
    let read = |did: identity::Did| {
        query::QueryRequest::new("{ Record { value } }").with_identity(Some(did))
    };
    let before = node.execute_request(read(target_did.clone())).await;
    assert_eq!(before.data.unwrap()["Record"].as_array().unwrap().len(), 0);
    for existed in [false, true] {
        let native_existed = node
            .add_dac_actor_relationship(
                owner_did.as_str(),
                "Record",
                &native_doc,
                "reader",
                target_did.as_str(),
            )
            .await
            .unwrap();
        let response = client
            .post(&url)
            .header("Authorization", owner_auth.as_str())
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "grant failed: {}",
            response.status()
        );
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["ExistedAlready"], existed);
        cases.push(json!({"input":{"caller":owner_did.as_str(), "target":target_did.as_str(), "nativeAllowed":true, "nativeChanged":!native_existed}, "allowed":true, "changed":!value["ExistedAlready"].as_bool().unwrap()}));
    }
    let after = node.execute_request(read(target_did.clone())).await;
    assert_eq!(after.data.unwrap()["Record"].as_array().unwrap().len(), 2);
    let preview_url = format!("http://{address}/api/v0/acp/document/decide");
    let preview_body = json!({
        "actor": target_did.as_str(), "permission": "read", "policy_id": policy,
        "resource_name": "records", "doc_id": body["doc_id"],
    });
    let denied_preview = client
        .post(&preview_url)
        .header("Authorization", bearer(&stranger, &address.to_string()))
        .json(&preview_body)
        .send()
        .await
        .unwrap();
    assert!(!denied_preview.status().is_success());
    let preview = client
        .post(&preview_url)
        .header("Authorization", owner_auth.as_str())
        .json(&preview_body)
        .send()
        .await
        .unwrap();
    assert!(preview.status().is_success());
    let preview: Value = preview.json().await.unwrap();
    assert_eq!(preview["allowed"], true);
    let removed = client
        .delete(&url)
        .header("Authorization", owner_auth.as_str())
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(
        removed.status().is_success(),
        "delete failed: {}",
        removed.status()
    );
    let after_delete = node.execute_request(read(target_did)).await;
    assert_eq!(
        after_delete.data.unwrap()["Record"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    node.shutdown().await.unwrap();
    cases
}

#[test]
fn embedded_http_relationships_use_native_caller_and_authorization() {
    edge::testing::edge()
        .block_on(exercise_http_relationships())
        .unwrap();
}

async fn exercise_transactional_http_relationships() -> Vec<Value> {
    let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
    let owner = identity();
    let owner_did = owner.did().unwrap();
    let target = identity();
    let target_did = target.did().unwrap();
    let mut owner_cases = Vec::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let node = Node::builder()
        .with_http(HttpConfig::with_listener(listener).unwrap())
        .build()
        .await
        .unwrap();
    let context = defra_core::RequestContext::default().with_identity(Some(owner_did.to_string()));
    let policy = node.add_dac_policy_as(&context, POLICY).await.unwrap();
    node.add_collection(&format!(
        "type Record @policy(id: \"{policy}\", resource: \"records\") {{ value: String }}"
    ))
    .await
    .unwrap();
    let created = node
        .execute_request(
            query::QueryRequest::new(
                r#"mutation { add_Record(input: {value: "secret"}) { _docID } }"#,
            )
            .with_identity(Some(owner_did.clone())),
        )
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    let doc = created.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    let client = net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let auth = bearer(&owner, &address.to_string());
    let other_auth = bearer(&target, &address.to_string());
    let tx_url = format!("http://{address}/api/v0/tx");
    let relationship_url = format!("http://{address}/api/v0/acp/document/relationship");
    let ownership_url = format!("http://{address}/api/v0/acp/document/is-owner");
    let relationship = json!({"collection":"Record", "doc_id":doc, "relation":"reader", "target_actor":target_did.as_str()});
    let ownership = json!({"collection":"Record", "doc_ids":[doc]});
    let deleted = node
        .execute_request(
            query::QueryRequest::new(
                r#"mutation { add_Record(input: {value: "deleted"}) { _docID } }"#,
            )
            .with_identity(Some(owner_did.clone())),
        )
        .await;
    assert!(!deleted.has_errors(), "{:?}", deleted.errors);
    let missing_doc = deleted.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    let deleted = node
        .execute_request(
            query::QueryRequest::new(format!(
                r#"mutation {{ delete_Record(docID: "{missing_doc}") {{ _docID }} }}"#
            ))
            .with_identity(Some(owner_did.clone())),
        )
        .await;
    assert!(!deleted.has_errors(), "{:?}", deleted.errors);

    let missing_tx = client
        .post(&ownership_url)
        .header("Authorization", auth.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    assert!(!missing_tx.status().is_success());
    let anonymous = client
        .post(&ownership_url)
        .json(&ownership)
        .send()
        .await
        .unwrap();
    assert!(!anonymous.status().is_success());

    let response = client
        .post(&tx_url)
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap();
    let other_tx = response.json::<Value>().await.unwrap()["id"]
        .as_u64()
        .unwrap()
        .to_string();
    let hidden = client
        .post(&ownership_url)
        .header("Authorization", other_auth.as_str())
        .header("x-defradb-tx", other_tx.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    assert!(hidden.status().is_success());
    let hidden: Value = hidden.json().await.unwrap();
    assert_eq!(hidden["documents"][0]["visible"], false);
    assert_eq!(hidden["documents"][0]["is_owner"], false);
    assert_eq!(hidden["documents"][0]["owner_registered"], false);
    owner_cases.push(owner_observation_case(
        false,
        true,
        false,
        &hidden["documents"][0],
    ));
    let missing = client
        .post(&ownership_url)
        .header("Authorization", other_auth.as_str())
        .header("x-defradb-tx", other_tx.as_str())
        .json(&json!({"collection":"Record", "doc_ids":[missing_doc]}))
        .send()
        .await
        .unwrap();
    assert!(missing.status().is_success());
    let missing: Value = missing.json().await.unwrap();
    assert_eq!(missing["documents"][0]["visible"], false);
    assert_eq!(missing["documents"][0]["owner_registered"], false);
    assert_eq!(missing["documents"][0]["is_owner"], false);
    owner_cases.push(owner_observation_case(
        false,
        false,
        false,
        &missing["documents"][0],
    ));
    assert!(client
        .delete(format!("{tx_url}/{other_tx}"))
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    for commit in [false, true] {
        let response = client
            .post(&tx_url)
            .header("Authorization", auth.as_str())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let body: Value = response.json().await.unwrap();
        let tx = body["id"].as_u64().unwrap().to_string();
        let owner_result = client
            .post(&ownership_url)
            .header("Authorization", auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&ownership)
            .send()
            .await
            .unwrap();
        assert!(owner_result.status().is_success());
        let body: Value = owner_result.json().await.unwrap();
        assert_eq!(body["documents"][0]["visible"], true);
        assert_eq!(body["documents"][0]["is_owner"], true);
        assert_eq!(body["documents"][0]["owner_registered"], true);
        assert!(body["documents"][0].get("owner").is_none());
        owner_cases.push(owner_observation_case(
            true,
            true,
            true,
            &body["documents"][0],
        ));
        let wrong_actor = client
            .post(&ownership_url)
            .header("Authorization", other_auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&ownership)
            .send()
            .await
            .unwrap();
        assert!(!wrong_actor.status().is_success());
        let wrong_grant = client
            .post(&relationship_url)
            .header("Authorization", other_auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&relationship)
            .send()
            .await
            .unwrap();
        assert!(!wrong_grant.status().is_success());
        let forged_actor = client
            .post(&ownership_url)
            .header("Authorization", auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&json!({"collection":"Record", "doc_ids":[doc], "actor":target_did.as_str()}))
            .send()
            .await
            .unwrap();
        assert!(!forged_actor.status().is_success());

        let grant = client
            .post(&relationship_url)
            .header("Authorization", auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&relationship)
            .send()
            .await
            .unwrap();
        assert!(
            grant.status().is_success(),
            "grant failed: {}",
            grant.status()
        );
        let read = node
            .execute_request(
                query::QueryRequest::new("{ Record { value } }")
                    .with_identity(Some(target_did.clone())),
            )
            .await;
        assert_eq!(read.data.unwrap()["Record"].as_array().unwrap().len(), 0);
        let final_url = format!("{tx_url}/{tx}");
        let final_result = if commit {
            client
                .post(&final_url)
                .header("Authorization", auth.as_str())
                .send()
                .await
                .unwrap()
        } else {
            client
                .delete(&final_url)
                .header("Authorization", auth.as_str())
                .send()
                .await
                .unwrap()
        };
        assert!(final_result.status().is_success());
    }
    let read = node
        .execute_request(
            query::QueryRequest::new("{ Record { value } }")
                .with_identity(Some(target_did.clone())),
        )
        .await;
    assert_eq!(read.data.unwrap()["Record"].as_array().unwrap().len(), 1);
    let response = client
        .post(&tx_url)
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap();
    let other_tx = response.json::<Value>().await.unwrap()["id"]
        .as_u64()
        .unwrap()
        .to_string();
    let nonowner = client
        .post(&ownership_url)
        .header("Authorization", other_auth.as_str())
        .header("x-defradb-tx", other_tx.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    assert!(nonowner.status().is_success());
    let nonowner: Value = nonowner.json().await.unwrap();
    assert_eq!(nonowner["documents"][0]["visible"], true);
    assert_eq!(nonowner["documents"][0]["is_owner"], false);
    assert_eq!(nonowner["documents"][0]["owner_registered"], true);
    assert!(nonowner["documents"][0].get("owner").is_none());
    owner_cases.push(owner_observation_case(
        true,
        true,
        false,
        &nonowner["documents"][0],
    ));

    // Revocation in another transaction cannot rewrite the snapshot of this observation.
    let revoked = client
        .delete(&relationship_url)
        .header("Authorization", auth.as_str())
        .json(&relationship)
        .send()
        .await
        .unwrap();
    assert!(revoked.status().is_success());
    let snapshot = client
        .post(&ownership_url)
        .header("Authorization", other_auth.as_str())
        .header("x-defradb-tx", other_tx.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    assert!(snapshot.status().is_success());
    let snapshot: Value = snapshot.json().await.unwrap();
    assert_eq!(snapshot["documents"][0]["visible"], true);
    assert_eq!(snapshot["documents"][0]["owner_registered"], true);
    owner_cases.push(owner_observation_case(
        true,
        true,
        false,
        &snapshot["documents"][0],
    ));
    assert!(client
        .delete(format!("{tx_url}/{other_tx}"))
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let response = client
        .post(&tx_url)
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap();
    let revoked_tx = response.json::<Value>().await.unwrap()["id"]
        .as_u64()
        .unwrap()
        .to_string();
    let after = client
        .post(&ownership_url)
        .header("Authorization", other_auth.as_str())
        .header("x-defradb-tx", revoked_tx.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    let after: Value = after.json().await.unwrap();
    assert_eq!(after["documents"][0]["visible"], false);
    assert_eq!(after["documents"][0]["owner_registered"], false);
    owner_cases.push(owner_observation_case(
        false,
        true,
        false,
        &after["documents"][0],
    ));
    assert!(client
        .delete(format!("{tx_url}/{revoked_tx}"))
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    let acp = node.document_acp();
    let unregistered_doc = doc.clone();
    node.on_acp(move || async move {
        acp.unregister_doc_object(&policy, "records", &unregistered_doc)
            .await?;
        Ok(())
    })
    .await
    .unwrap();
    let response = client
        .post(&tx_url)
        .header("Authorization", auth.as_str())
        .send()
        .await
        .unwrap();
    let unregistered_tx = response.json::<Value>().await.unwrap()["id"]
        .as_u64()
        .unwrap()
        .to_string();
    let unregistered = client
        .post(&ownership_url)
        .header("Authorization", auth.as_str())
        .header("x-defradb-tx", unregistered_tx.as_str())
        .json(&ownership)
        .send()
        .await
        .unwrap();
    let unregistered: Value = unregistered.json().await.unwrap();
    assert_eq!(unregistered["documents"][0]["visible"], true);
    assert_eq!(unregistered["documents"][0]["owner_registered"], false);
    assert_eq!(unregistered["documents"][0]["is_owner"], false);
    owner_cases.push(owner_observation_case(
        true,
        false,
        false,
        &unregistered["documents"][0],
    ));
    assert!(client
        .delete(format!("{tx_url}/{unregistered_tx}"))
        .header("Authorization", auth.as_str())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    node.shutdown().await.unwrap();
    owner_cases
}

fn owner_observation_case(
    visible: bool,
    owner_present: bool,
    caller_is_owner: bool,
    row: &Value,
) -> Value {
    let keys: std::collections::BTreeSet<_> = row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["doc_id", "is_owner", "owner_registered", "visible"]
            .into_iter()
            .collect(),
        "owner identity must not be disclosed"
    );
    json!({"input": {"nativeVisible": visible, "ownerPresent": owner_present, "callerIsOwner": caller_is_owner},
        "visible":row["visible"], "owner_registered":row["owner_registered"], "is_owner":row["is_owner"]})
}

#[test]
fn http_relationships_and_owner_observation_use_the_authenticated_transaction() {
    edge::testing::edge()
        .block_on(exercise_transactional_http_relationships())
        .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn http_owner_registration_matches_executed_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let cases = edge::testing::edge()
        .block_on(exercise_transactional_http_relationships())
        .unwrap();
    let inputs: Vec<_> = cases.iter().map(|case| case["input"].clone()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "HttpOwnerRegistration.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases, expected);
}

#[test]
fn http_cannot_use_the_embedder_only_transaction_registry() {
    edge::testing::edge()
        .block_on(async {
            let local = Node::builder()
                .with_txn_ownership(db::TxnOwnership::Embedder)
                .build()
                .await
                .unwrap();
            local.shutdown().await.unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let exposed = Node::builder()
                .with_txn_ownership(db::TxnOwnership::Embedder)
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await;
            assert!(matches!(
                exposed,
                Err(crate::Error::HttpEmbedderTransactionOwnership)
            ));
        })
        .unwrap();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn embedded_http_relationships_match_executed_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let cases = edge::testing::edge()
        .block_on(exercise_http_relationships())
        .unwrap();
    let inputs: Vec<_> = cases.iter().map(|case| case["input"].clone()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "EmbeddedHttpAcp.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases, expected);
}

#[test]
fn source_permission_http_uses_caller_transaction() {
    edge::testing::edge().block_on(async {
        let owner = identity();
        let owner_did = owner.did().unwrap();
        let actor = identity().did().unwrap();
        let outsider = identity();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().with_node_acp_owner(owner_did.as_str())
            .with_http(HttpConfig::with_listener(listener).unwrap()).build().await.unwrap();
        let client = net::http::Client::builder(edge::testing::edge()).no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
        let auth = bearer(&owner, &address.to_string());
        let body = json!({"permission":"patch-collection","parent_source":null,"subject_kind":"source","subject":"tools:one","delete":false});
        let url = format!("http://{address}/api/v0/acp/node/source-permission");
        let missing = client.post(&url).header("Authorization", &auth).json(&body).send().await.unwrap();
        assert!(!missing.status().is_success());
        let begin: Value = client.post(format!("http://{address}/api/v0/tx"))
            .header("Authorization", &auth).send().await.unwrap().json().await.unwrap();
        let tx = begin["id"].as_u64().unwrap().to_string();
        let wrong = client.post(&url).header("Authorization", bearer(&outsider, &address.to_string()))
            .header("x-defradb-tx", &tx).json(&body).send().await.unwrap();
        assert!(!wrong.status().is_success());
        let staged = client.post(&url).header("Authorization", &auth)
            .header("x-defradb-tx", &tx).json(&body).send().await.unwrap();
        assert!(staged.status().is_success(), "{}", staged.status());
        let member = json!({"permission":"patch-collection","parent_source":"tools:one","subject_kind":"actor","subject":actor.as_str(),"delete":false});
        let staged = client.post(&url).header("Authorization", &auth)
            .header("x-defradb-tx", &tx).json(&member).send().await.unwrap();
        assert!(staged.status().is_success(), "{}", staged.status());
        assert!(!node.nac_manager().check_permission(&actor, acp::NodePermission::CollectionPatch).await.unwrap());
        client.post(format!("http://{address}/api/v0/tx/{tx}"))
            .header("Authorization", &auth).send().await.unwrap().error_for_status().unwrap();
        assert!(node.nac_manager().check_permission(&actor, acp::NodePermission::CollectionPatch).await.unwrap());
        node.shutdown().await.unwrap();
    }).unwrap();
}

mod bound_caller {
    use std::sync::Arc;

    use defra_core::RequestContext;
    use defra_http::DocumentAcpOperations;
    use identity::Did;
    use regolith::sync::Mutex;

    use crate::acp_ops::AcpOps;
    use crate::ops::HttpNodeOps;
    use crate::Node;

    struct RecordContext(Mutex<Vec<(String, Option<String>)>>);

    #[async_trait::async_trait]
    impl AcpOps for RecordContext {
        async fn check_doc_access(
            &self,
            _context: &RequestContext,
            _actor: &Did,
            _permission: acp::DocumentPermission,
            _policy_id: &str,
            _resource: &str,
            _doc: &str,
        ) -> crate::Result<bool> {
            Ok(true)
        }

        async fn add_dac_actor_relationship(
            &self,
            context: &RequestContext,
            requestor: &str,
            _collection: &str,
            _doc: &str,
            _relation: &str,
            _target: &str,
        ) -> crate::Result<bool> {
            self.0
                .lock()
                .await
                .push((requestor.to_owned(), context.bearer_token_for(requestor)));
            Ok(false)
        }

        async fn delete_dac_actor_relationship(
            &self,
            context: &RequestContext,
            requestor: &str,
            collection: &str,
            doc: &str,
            relation: &str,
            target: &str,
        ) -> crate::Result<bool> {
            self.add_dac_actor_relationship(context, requestor, collection, doc, relation, target)
                .await
        }
    }

    #[test]
    fn relationship_calls_preserve_the_bound_callers_bearer() {
        workers::block_on(async {
            let node = Node::builder().build().await.unwrap();
            let recorder = Arc::new(RecordContext(Mutex::new(Vec::new())));
            let mut ops = node.ops();
            ops.acp_ops = recorder.clone();
            let adapter = HttpNodeOps::new(ops);
            let caller = Did::new("did:key:caller").unwrap();
            let context = RequestContext::default()
                .with_identity(Some(caller.to_string()))
                .with_bearer(Some(caller.to_string()), Some("caller-token".into()));
            let bound = adapter.bind_request(&context).unwrap();
            bound
                .add_doc_relationship(Some(&caller), "did:key:target", "Record", "doc", "reader")
                .await
                .unwrap();
            let other = Did::new("did:key:other").unwrap();
            assert!(bound
                .add_doc_relationship(Some(&other), "did:key:target", "Record", "doc", "reader")
                .await
                .is_err());
            assert_eq!(
                *recorder.0.lock().await,
                vec![(caller.to_string(), Some("caller-token".into()))]
            );
            node.shutdown().await.unwrap();
        })
        .unwrap();
    }
}

/// With node ACP on, the embedded server checks the routes that decide access
/// at the edge, as `defradb start` does: a caller without `DocumentRead`
/// cannot stream document updates.
#[test]
fn embedded_events_require_node_permission() {
    edge::testing::edge()
        .block_on(async {
            let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
            let owner = identity();
            let owner_did = owner.did().unwrap();
            let public = owner.public_key_bytes();
            defra_core::signing::store_identity(
                owner_did.as_str(),
                SigningConfig {
                    key_type: SigningKeyType::Ed25519,
                    private_key_bytes: owner.private_key_bytes(),
                    public_key_bytes: public.clone(),
                    public_key_hex: hex::encode(public),
                    remote_signer: None,
                    signing_authorization: None,
                },
            );
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let node = Node::builder()
                .with_node_identity_did(owner_did.as_str())
                .with_node_acp_enabled()
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            node.add_collection("type Note { text: String }")
                .await
                .unwrap();
            let client = net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let events = format!("http://{address}/api/v0/events?event=update");
            let anonymous = client.get(&events).send().await.unwrap();
            assert_refused(anonymous, "read-document").await;
            let owner_stream = client
                .get(&events)
                .header("Authorization", bearer(&owner, &address.to_string()))
                .send()
                .await
                .unwrap();
            assert_eq!(
                owner_stream.status().as_u16(),
                200,
                "the owner could not stream updates"
            );
            let peer_info = client
                .get(format!("http://{address}/api/v0/p2p/info"))
                .send()
                .await
                .unwrap();
            assert_refused(peer_info, "get-p2p-peer-info").await;
        })
        .unwrap();
}

/// Asserts the edge's node ACP refusal: 401, naming `permission` as Go does.
async fn assert_refused(response: net::http::Response, permission: &str) {
    assert_eq!(response.status().as_u16(), 401, "not refused by node ACP");
    let body = response.text().await.unwrap();
    assert!(
        body.contains(&format!("Permission: {permission}")),
        "the refusal does not name {permission}: {body}"
    );
}

fn store_signing_identity(owner: &RawIdentity) -> String {
    let owner_did = owner.did().unwrap();
    let public = owner.public_key_bytes();
    defra_core::signing::store_identity(
        owner_did.as_str(),
        SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: owner.private_key_bytes(),
            public_key_bytes: public.clone(),
            public_key_hex: hex::encode(public),
            remote_signer: None,
            signing_authorization: None,
        },
    );
    owner_did.as_str().to_owned()
}

fn http_client() -> net::http::Client {
    net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

/// A store a previous run enabled node ACP in keeps it enabled on restart, so
/// the embedded server keeps checking the edge routes even when this run did
/// not ask for node ACP.
#[test]
fn embedded_edge_stays_checked_after_restart_without_the_flag() {
    edge::testing::edge()
        .block_on(async {
            let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
            let owner_did = store_signing_identity(&identity());
            let directory = tempfile::tempdir().unwrap();
            let first = Node::builder()
                .data_path(directory.path())
                .with_node_identity_did(&owner_did)
                .with_node_acp_enabled()
                .build()
                .await
                .unwrap();
            first.shutdown().await.unwrap();
            drop(first);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let reopened = Node::builder()
                .data_path(directory.path())
                .with_node_identity_did(&owner_did)
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            let response = http_client()
                .get(format!("http://{address}/api/v0/p2p/info"))
                .send()
                .await
                .unwrap();
            assert_refused(response, "get-p2p-peer-info").await;
            reopened.shutdown().await.unwrap();
        })
        .unwrap();
}

/// An embedder's own routes are not node routes: the edge's node ACP check,
/// and its `DocumentRead` default for routes outside the table, stay off them.
#[test]
fn embedded_extra_routes_are_not_node_checked() {
    edge::testing::edge()
        .block_on(async {
            let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
            let owner_did = store_signing_identity(&identity());
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let extra =
                axum::Router::new().route("/app/hello", axum::routing::get(|| async { "hello" }));
            let node = Node::builder()
                .with_node_identity_did(&owner_did)
                .with_node_acp_enabled()
                .with_http(
                    HttpConfig::with_listener(listener)
                        .unwrap()
                        .with_extra_routes(extra),
                )
                .build()
                .await
                .unwrap();
            let response = http_client()
                .get(format!("http://{address}/app/hello"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status().as_u16(),
                200,
                "an embedder route was gated"
            );
            assert_eq!(response.text().await.unwrap(), "hello");
            node.shutdown().await.unwrap();
        })
        .unwrap();
}
