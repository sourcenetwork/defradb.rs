//! The HTTP server signs an anonymous write as the node, whichever way the
//! node's signing identity was set up.

use std::time::Duration;

use serde_json::{json, Value};

use crate::{HttpConfig, Node};

#[test]
fn http_write_signs_as_a_generated_node_identity() {
    edge::testing::edge()
        .block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let node = Node::builder()
                .enable_signing()
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            node.add_collection("type User { name: String }")
                .await
                .unwrap();
            let client = net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let mutation = r#"mutation { add_User(input: {name: "a"}) { _version { cid } } }"#;
            let response: Value = client
                .post(format!("http://{address}/api/v0/graphql"))
                .json(&json!({ "query": mutation }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let cid = response["data"]["add_User"][0]["_version"][0]["cid"]
                .as_str()
                .unwrap_or_else(|| panic!("no block CID in {response}"))
                .to_owned();
            let signer = node.verified_block_signer_did(&cid).await.unwrap();
            assert_eq!(Some(signer.as_str()), node.node_identity_did());
            node.shutdown().await.unwrap();
        })
        .unwrap();
}

#[test]
fn http_client_signs_the_complete_agent_create_graph() {
    use crypto::{Key as _, PrivateKey as _};
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use document::{Document, NormalValue};
    use identity::Identity as _;

    edge::testing::edge().block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().enable_signing()
            .with_http(HttpConfig::with_listener(listener).unwrap())
            .build().await.unwrap();
        node.add_collection("type Agent { agent_did: String @immutable, creation_nonce: String @immutable, node_did: String @immutable, display_name: String, tags: [String!] }").await.unwrap();
        let schema = node.get_collection("Agent").unwrap().unwrap();
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let identity = identity::RawIdentity::from_private_key(private).unwrap();
        let caller = identity.did().unwrap().to_string();
        let token = String::from_utf8(identity::new_token(&identity, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let authorization = format!("Bearer {token}");
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        for explicit in [false, true] {
            let mut doc = Document::new(schema.clone());
            doc.set("agent_did", NormalValue::String("did:key:agent".into())).unwrap();
            doc.set("creation_nonce", NormalValue::String(format!("nonce-{explicit}"))).unwrap();
            doc.set("node_did", NormalValue::String("did:key:node".into())).unwrap();
            doc.set("display_name", NormalValue::String("Example".into())).unwrap();
            doc.set("tags", NormalValue::NillableStringArray(Some(vec!["one".into(), "two".into()]))).unwrap();
            let alias = if explicit { "created: " } else { "" };
            let request = query::QueryRequest::new(format!("mutation($input: AgentMutationInputArg!) {{ {alias}create_Agent(input: $input) {{ _docID _version {{ cid }} }} }}"))
                .with_variables(json!({"input": doc.to_map().unwrap()}));
            let mut graph = db::block::builder::ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signing).unwrap();
            for selection in ["display_name", "id: _docID", "_docID _docID: display_name"] {
                let invalid = query::QueryRequest::new(format!("mutation($input: AgentMutationInputArg!) {{ create_Agent(input: $input) {{ {selection} }} }}"))
                    .with_variables(json!({"input": doc.to_map().unwrap()}));
                let error = defra_http::client_signed_create::execute(
                    "ws://127.0.0.1:1/api/v0/graphql/client-sign", &authorization, invalid, None, &mut graph,
                ).await.unwrap_err();
                assert!(matches!(error, defra_http::client_signed_create::ClientSignedCreateError::InputMismatch), "{error:?}");
            }
            let txn = if explicit {
                let response: Value = http.post(format!("http://{address}/api/v0/tx"))
                    .bearer_auth(&token).send().await.unwrap().json().await.unwrap();
                Some(response["id"].as_u64().unwrap().to_string())
            } else { None };
            let response = defra_http::client_signed_create::execute(
                &format!("ws://{address}/api/v0/graphql/client-sign"), &authorization, request, txn.as_deref(), &mut graph,
            ).await.unwrap();
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            let data = response.data.unwrap();
            let root = if explicit { "created" } else { "add_Agent" };
            let cid = data[root][0]["_version"][0]["cid"].as_str().unwrap_or_else(|| panic!("unexpected create response: {data}"));
            if let Some(txn) = txn {
                assert!(node.verified_block_signer_did(cid).await.is_err());
                http.post(format!("http://{address}/api/v0/tx/{txn}"))
                    .bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
            }
            assert_eq!(node.verified_block_signer_did(cid).await.unwrap(), caller);
            assert_ne!(Some(caller.as_str()), node.node_identity_did());
        }
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[tokio::test]
async fn client_signed_gents_agent_commits_before_http_relationship_grants() {
    use crypto::{Key as _, PrivateKey as _};
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use identity::Identity as _;

    const POLICY: &str = r#"
name: Gents identity ownership model
resources:
  - name: nodes
    relations:
      - name: administrator
        types: [actor]
        manages: [administrator, runtime_reader]
      - name: runtime_reader
        types: [actor]
    permissions:
      - name: read
        expr: administrator + runtime_reader
      - name: update
        expr: administrator
      - name: delete
        expr: administrator
  - name: agents
    relations:
      - name: parent
        types: [nodes]
      - name: reader
        types: [actor]
      - name: session_writer
        types: [actor]
      - name: administrator
        types: [actor]
        manages: [parent, reader, session_writer, administrator, executor, config_writer]
      - name: executor
        types: [actor]
      - name: config_writer
        types: [actor]
    permissions:
      - name: read
        expr: reader + session_writer + administrator + executor + config_writer + parent->read
      - name: update
        expr: administrator + config_writer + parent->update
      - name: delete
        expr: administrator + parent->delete
      - name: invoke
        expr: session_writer + executor + administrator + parent->delete
      - name: session_read
        expr: reader + session_writer + administrator + parent->delete
      - name: session_admin
        expr: administrator + parent->delete
"#;
    let private = crypto::generate_ed25519().unwrap();
    let public = private.public_key();
    let signer = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: private.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    let identity = identity::RawIdentity::from_private_key(private).unwrap();
    let caller = identity.did().unwrap().to_string();
    let edge = edge::testing::edge();
    let owner = caller.clone();
    let (node, schema, address, parent_id, node_did, token, txn) = edge
        .spawn(async move {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let node = Node::builder()
                .enable_signing()
                .with_node_acp_owner(owner.clone())
                .with_http(HttpConfig::with_listener(listener).unwrap())
                .build()
                .await
                .unwrap();
            let node_did = node.node_identity_did().unwrap().to_owned();
            let context = defra_core::RequestContext::default().with_identity(Some(owner.clone()));
            let policy = node.add_dac_policy_as(&context, POLICY).await.unwrap();
            node.add_collection(&format!(
                "type Node @policy(id: \"{policy}\", resource: \"nodes\") {{ node_did: String }}"
            ))
            .await
            .unwrap();
            node.add_collection(&format!(
                r#"type Agent @policy(id: "{policy}", resource: "agents") {{
            agent_did: String @index(unique: true) @immutable
            creation_nonce: String @immutable
            node_did: String @index @immutable
            display_name: String
            description: String
            context_id: String @index
            inference_profile_id: String @index
            enabled: Boolean @index
            tags: [String!]
            created_at: String
            updated_at: DateTime @index(direction: DESC)
        }}"#
            ))
            .await
            .unwrap();
            let parent = node
                .execute_request(
                    query::QueryRequest::new(
                        "mutation($did:String!) { add_Node(input: {node_did:$did}) { _docID } }",
                    )
                    .with_variables(json!({"did":node_did}))
                    .with_identity(Some(identity::Did::new(owner.clone()).unwrap())),
                )
                .await;
            assert!(!parent.has_errors(), "{:?}", parent.errors);
            let parent_id = parent.data.unwrap()["add_Node"][0]["_docID"]
                .as_str()
                .unwrap()
                .to_owned();
            let schema = node.get_collection("Agent").unwrap().unwrap();
            let token = String::from_utf8(
                identity::new_token(
                    &identity,
                    Duration::from_secs(60),
                    Some(address.to_string()),
                    None,
                )
                .unwrap(),
            )
            .unwrap();
            let client = net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .build()
                .unwrap();
            let txn: Value = client
                .post(format!("http://{address}/api/v0/tx"))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            (
                node,
                schema,
                address,
                parent_id,
                node_did,
                token,
                txn["id"].as_u64().unwrap().to_string(),
            )
        })
        .await
        .unwrap();
    let input = json!({"agent_did":"did:key:agent", "creation_nonce":"nonce", "node_did":node_did, "display_name":"Example", "description":null, "context_id":null, "inference_profile_id":"profile", "enabled":true, "tags":["one","two"], "created_at":null, "updated_at":"2026-08-14T12:00:00.120000000Z"});
    let mut graph = db::block::builder::ClientCreateSignatures::from_input(
        &storage::Crdts::default(),
        &schema,
        &input,
        &signer,
    )
    .unwrap();
    let request = query::QueryRequest::new("mutation($input: AgentMutationInputArg!) { add_Agent(input:$input) { _docID _version { cid } } }").with_variables(json!({"input":input}));
    let handshake = defra_http::client_signed_create::websocket_request(
        &format!("ws://{address}/api/v0/graphql/client-sign"),
        &format!("Bearer {token}"),
    )
    .unwrap();
    let (socket, _) = tokio_tungstenite::connect_async_with_config(
        handshake,
        Some(defra_http::client_signed_create::websocket_config()),
        false,
    )
    .await
    .unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(90),
        defra_http::client_signed_create::execute_with_socket(
            socket,
            request,
            Some(&txn),
            &mut graph,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.unwrap();
    let doc_id = data["add_Agent"][0]["_docID"].as_str().unwrap().to_owned();
    let cid = data["add_Agent"][0]["_version"][0]["cid"]
        .as_str()
        .unwrap()
        .to_owned();
    edge.spawn(async move {
        let read = || query::QueryRequest::new("{ Agent { _docID description context_id tags updated_at } }")
            .with_identity(Some(identity::Did::new(caller.clone()).unwrap()));
        let before = node.execute_request(read()).await;
        assert!(!before.has_errors(), "{:?}", before.errors);
        assert!(before.data.unwrap()["Agent"].as_array().unwrap().is_empty());
        let client = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        client.post(format!("http://{address}/api/v0/tx/{txn}")).bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        let after = node.execute_request(read()).await;
        assert!(!after.has_errors(), "{:?}", after.errors);
        let rows = after.data.unwrap();
        assert_eq!(rows["Agent"][0]["_docID"], doc_id);
        assert_eq!(rows["Agent"][0]["description"], Value::Null);
        assert_eq!(rows["Agent"][0]["context_id"], Value::Null);
        assert_eq!(rows["Agent"][0]["tags"], json!(["one", "two"]));
        assert_eq!(rows["Agent"][0]["updated_at"], "2026-08-14T12:00:00.12Z");
        let grants = [("parent", format!("nodes:\"{parent_id}\"")), ("executor", node_did)];
        for (index, (relation, target)) in grants.iter().enumerate() {
            if index == 1 {
                let failed = client.post(format!("http://{address}/api/v0/acp/document/relationship")).bearer_auth(&token)
                    .json(&json!({"collection":"Agent", "doc_id":doc_id, "relation":"executor", "target_actor":"not-a-did"})).send().await.unwrap();
                assert!(!failed.status().is_success());
                let still_present = node.execute_request(read()).await;
                assert_eq!(still_present.data.unwrap()["Agent"][0]["_docID"], doc_id);
            }
            let response = client.post(format!("http://{address}/api/v0/acp/document/relationship")).bearer_auth(&token)
                .json(&json!({"collection":"Agent", "doc_id":doc_id, "relation":relation, "target_actor":target})).send().await.unwrap();
            assert!(response.status().is_success(), "{relation} grant: {}", response.status());
            let outcome: Value = response.json().await.unwrap();
            assert_eq!(outcome["ExistedAlready"], false);
        }
        for (relation, target) in &grants {
            let response = client.post(format!("http://{address}/api/v0/acp/document/relationship")).bearer_auth(&token)
                .json(&json!({"collection":"Agent", "doc_id":doc_id, "relation":relation, "target_actor":target})).send().await.unwrap();
            assert!(response.status().is_success(), "reconcile {relation}: {}", response.status());
            let outcome: Value = response.json().await.unwrap();
            assert_eq!(outcome["ExistedAlready"], true);
        }
        assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), caller);
        node.shutdown().await.unwrap();
    }).await.unwrap();
}
