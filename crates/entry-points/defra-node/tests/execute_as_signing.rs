use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use defra_node::{Node, ScopedNode, SigningKey};
use identity::{Identity as _, RawIdentity};

fn identity_from_key(
    private_key: crypto::keys::ed25519::Ed25519PrivateKey,
) -> (identity::Did, String, Vec<u8>, Vec<u8>) {
    let raw = RawIdentity::from_ed25519(private_key).expect("build raw identity");
    let did = raw.did().expect("derive DID");
    let identity = hex::encode(raw.public_key_bytes());
    let private_key_bytes = raw.private_key_bytes().to_vec();
    (
        did,
        identity,
        private_key_bytes,
        raw.public_key_bytes().to_vec(),
    )
}

fn signing_config(private_key_bytes: Vec<u8>, public_key_bytes: Vec<u8>) -> SigningConfig {
    SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes,
        public_key_bytes: public_key_bytes.clone(),
        public_key_hex: hex::encode(&public_key_bytes),
        remote_signer: None,
        signing_authorization: None,
    }
}

fn document_id(response: &query::QueryResponse) -> String {
    let data = response.data.as_ref().expect("create response data");
    data.pointer("/add_Widget/0/_docID")
        .or_else(|| data.pointer("/create_Widget/0/_docID"))
        .and_then(serde_json::Value::as_str)
        .expect("created document ID")
        .to_string()
}

async fn create(scope: &ScopedNode<'_>, name: &str) -> String {
    let response = scope
        .execute(&format!(
            r#"mutation {{ add_Widget(input: {{name: "{name}"}}) {{ _docID }} }}"#
        ))
        .await;
    assert!(
        response.errors.is_empty(),
        "create failed: {:?}",
        response.errors
    );
    document_id(&response)
}

async fn commit_signatures(scope: &ScopedNode<'_>, doc_id: &str) -> Vec<serde_json::Value> {
    let response = scope
        .execute(&format!(
            r#"query {{ _commits(docID: "{doc_id}") {{ signature {{ type identity }} }} }}"#
        ))
        .await;
    assert!(
        response.errors.is_empty(),
        "commit query failed: {:?}",
        response.errors
    );
    response
        .data
        .as_ref()
        .and_then(|data| data.get("_commits"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("commit rows")
}

fn signed_by(rows: &[serde_json::Value], identity: &str) -> bool {
    !rows.is_empty()
        && rows.iter().all(|row| {
            row.pointer("/signature/type")
                .and_then(serde_json::Value::as_str)
                == Some("EdDSA")
                && row
                    .pointer("/signature/identity")
                    .and_then(serde_json::Value::as_str)
                    == Some(identity)
        })
}

fn unsigned(rows: &[serde_json::Value]) -> bool {
    !rows.is_empty()
        && rows
            .iter()
            .all(|row| row.get("signature").is_some_and(serde_json::Value::is_null))
}

#[test]
fn execute_as_signs_writes_and_keeps_the_caller_identity() {
    workers::block_on(async {
        defra_core::signing::clear_identity_store();
        let node_signing_key = crypto::generate_ed25519().unwrap();
        let (_, node_identity, node_private, _) = identity_from_key(node_signing_key);
        let (caller_did, caller_identity, caller_private, caller_public) =
            identity_from_key(crypto::generate_ed25519().unwrap());
        let caller_key = signing_config(caller_private, caller_public);
        defra_core::signing::store_identity(caller_did.as_ref(), caller_key.clone()).unwrap();

        let node = Node::builder()
            .with_signing_key(SigningKey::Ed25519(node_private))
            .build()
            .await
            .unwrap();
        node.add_collection("type Widget { name: String }")
            .await
            .unwrap();
        assert!(node.node_identity_did().is_some());
        assert!(
            defra_core::signing::get_identity(node.node_identity_did().unwrap()).is_some(),
            "node signer must be registered"
        );
        let ambient = RequestContext::ambient();
        assert!(
            defra_core::signing::resolve_signing_config_with_flag(
                ambient.identity(),
                node.node_identity_did(),
                true,
            )
            .is_some(),
            "the node signer must resolve"
        );

        let doc_id = create(&node.anonymous(), "node-signed").await;
        let rows = commit_signatures(&node.anonymous(), &doc_id).await;
        assert!(
            signed_by(&rows, &node_identity),
            "node-key writes must be signed by the node: {rows:?}"
        );

        // A key the process registered for the caller's DID is not the
        // caller's to sign with: without a key of its own the node signs.
        let caller = node.with_identity(identity::Did::new(caller_did.to_string()).unwrap());
        let doc_id = create(&caller, "registered-caller").await;
        let rows = commit_signatures(&caller, &doc_id).await;
        assert!(
            signed_by(&rows, &node_identity),
            "a caller without a key must be signed by the node: {rows:?}"
        );

        let keyed = caller.clone().with_signing(caller_key);
        let doc_id = create(&keyed, "caller-signed").await;
        let rows = commit_signatures(&caller, &doc_id).await;
        assert!(
            signed_by(&rows, &caller_identity),
            "a caller's own key must sign its writes: {rows:?}"
        );

        let regression_response = node
            .execute(r#"mutation { add_Widget(input: {name: "execute-signed"}) { _docID } }"#)
            .await;
        assert!(
            regression_response.errors.is_empty(),
            "execute create failed: {:?}",
            regression_response.errors
        );
        let regression_doc = document_id(&regression_response);
        let rows = commit_signatures(&node.anonymous(), &regression_doc).await;
        assert!(
            signed_by(&rows, &node_identity),
            "Node::execute writes must stay signed by the node: {rows:?}"
        );
        node.shutdown().await.unwrap();

        let unsigned_node = Node::builder().build().await.unwrap();
        unsigned_node
            .add_collection("type Widget { name: String }")
            .await
            .unwrap();
        let doc_id = create(&unsigned_node.anonymous(), "unsigned").await;
        let rows = commit_signatures(&unsigned_node.anonymous(), &doc_id).await;
        assert!(
            unsigned(&rows),
            "disabled signing must stay unsigned: {rows:?}"
        );
        unsigned_node.shutdown().await.unwrap();

        defra_core::signing::clear_identity_store();
    })
    .unwrap();
}
