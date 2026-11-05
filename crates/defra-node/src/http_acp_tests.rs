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
    );
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

async fn exercise_transactional_http_relationships() {
    let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
    let owner = identity();
    let owner_did = owner.did().unwrap();
    let target = identity();
    let target_did = target.did().unwrap();
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
        assert!(body["documents"][0].get("owner").is_none());
        let wrong_actor = client
            .post(&ownership_url)
            .header("Authorization", other_auth.as_str())
            .header("x-defradb-tx", tx.as_str())
            .json(&ownership)
            .send()
            .await
            .unwrap();
        assert!(!wrong_actor.status().is_success());
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
    assert!(client
        .delete(format!("{tx_url}/{other_tx}"))
        .header("Authorization", other_auth.as_str())
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    node.shutdown().await.unwrap();
}

#[test]
fn http_relationships_and_owner_observation_use_the_authenticated_transaction() {
    edge::testing::edge()
        .block_on(exercise_transactional_http_relationships())
        .unwrap();
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
