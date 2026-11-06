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
            let doc_id = data[root][0]["_docID"].as_str().unwrap();
            let history = defra_http::client_signed_update::history_request(doc_id);
            let response: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql"))
                .bearer_auth(&token).json(&json!({"query":history.query,"variables":history.variables}))
                .send().await.unwrap().json().await.unwrap();
            let snapshot = defra_http::client_signed_update::snapshot_from_response(doc_id, &response).unwrap();
            let patch = json!({"display_name":"Updated"});
            let mut update = db::block::builder::ClientUpdateSignatures::from_input(
                &storage::Crdts::default(), &schema, doc_id, &patch, &snapshot, &signing,
            ).unwrap();
            let request = query::QueryRequest::new("mutation($id:ID!, $input:AgentMutationInputArg!) { update_Agent(docID:$id,input:$input) { _docID _version { cid } } }")
                .with_variables(json!({"id":doc_id,"input":patch}));
            let updated = defra_http::client_signed_update::execute(
                &format!("ws://{address}/api/v0/graphql/client-sign"), &authorization, request, None, &mut update,
            ).await.unwrap();
            assert!(updated.errors.is_empty(), "{:?}", updated.errors);
            let cid = updated.data.unwrap()["update_Agent"][0]["_version"][0]["cid"].as_str().unwrap().to_owned();
            assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), caller);
            assert!(db::block::builder::ClientUpdateSignatures::from_input(
                &storage::Crdts::default(), &schema, doc_id, &json!({"node_did":"changed"}), &snapshot, &signing,
            ).is_err());
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

#[test]
fn http_client_signs_a_single_document_update() {
    use crypto::{Key as _, PrivateKey as _};
    use db::block::builder::{
        ClientCreateSignatures, ClientUpdateSignatures, ClientUpdateSnapshot,
    };
    use defra_core::block::{Block, Signature, SignatureHeader, SignatureType};
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use identity::Identity as _;

    edge::testing::edge().block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().enable_signing()
            .with_http(HttpConfig::with_listener(listener).unwrap())
            .build().await.unwrap();
        node.add_collection("type Settings { name: String, tags: [String!], host: JSON, extra: String }").await.unwrap();
        let schema = node.get_collection("Settings").unwrap().unwrap();
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
        let url = format!("ws://{address}/api/v0/graphql/client-sign");
        let initial = json!({"name":"before", "tags":["one"], "host":{"url":"before"}});
        let mut history_graph = ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &initial, &signing).unwrap();
        let children = history_graph.child_blocks().to_vec();
        let signatures = history_graph.respond(&children).unwrap();
        let signed = |bytes: &[u8], signature: Vec<u8>| {
            let mut block = Block::from_dag_cbor(bytes).unwrap();
            block.signature = Some(Signature::new(
                SignatureHeader::new(SignatureType::EdDSA, signing.public_key_hex.as_bytes().to_vec()), signature,
            ).generate_cid().unwrap());
            block.to_dag_cbor().unwrap()
        };
        let mut history = children.iter().zip(signatures).map(|(bytes, signature)| signed(bytes, signature)).collect::<Vec<_>>();
        let composite = history_graph.composite_block().to_vec();
        let signature = history_graph.respond(std::slice::from_ref(&composite)).unwrap().remove(0);
        let composite = signed(&composite, signature);
        let composite_cid = Block::from_dag_cbor(&composite).unwrap().generate_cid().unwrap();
        history.push(composite);
        let doc_id = history_graph.doc_id().to_owned();
        ClientUpdateSnapshot::from_blocks(&doc_id, &history, &[composite_cid]).unwrap();
        let mut create = ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &initial, &signing).unwrap();
        let created = defra_http::client_signed_create::execute(
            &url, &authorization,
            query::QueryRequest::new("mutation($input: SettingsMutationInputArg!) { add_Settings(input:$input) { _docID } }")
                .with_variables(json!({"input":initial})), None, &mut create,
        ).await.unwrap();
        assert!(created.errors.is_empty(), "{:?}", created.errors);
        assert_eq!(created.data.unwrap()["add_Settings"][0]["_docID"], doc_id);
        let query = defra_http::client_signed_update::history_request(&doc_id);
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        let response: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql"))
            .bearer_auth(&token).json(&json!({"query":query.query, "variables":query.variables}))
            .send().await.unwrap().json().await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let snapshot = defra_http::client_signed_update::snapshot_from_response(&doc_id, &response).unwrap();
        let mut forged = response.clone();
        forged.data.as_mut().unwrap()["_commits"][0]["height"] = json!(999);
        assert!(defra_http::client_signed_update::snapshot_from_response(&doc_id, &forged).is_err());

        let patch = json!({"name":"after", "tags":null, "host":{"url":"after"}});
        let mut graph = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &snapshot, &signing).unwrap();
        let request = query::QueryRequest::new("mutation Other { update_Settings(docID:\"invalid\", input:{name:\"x\"}) { _docID } } mutation Chosen($id:ID!, $input:SettingsMutationInputArg!) { changed: update_Settings(docID:$id, input:$input) { _docID _version { cid } } }")
            .with_operation_name("Chosen").with_variables(json!({"id":doc_id, "input":patch}));
        let response = defra_http::client_signed_update::execute(&url, &authorization, request, None, &mut graph).await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let data = response.data.unwrap();
        assert_eq!(data["changed"][0]["_docID"], doc_id);
        let cid = data["changed"][0]["_version"][0]["cid"].as_str().unwrap();
        assert_eq!(node.verified_block_signer_did(cid).await.unwrap(), caller);
        assert_ne!(Some(caller.as_str()), node.node_identity_did());
        let stale_patch = json!({"name":"stale-attempt"});
        let mut stale = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &stale_patch, &snapshot, &signing).unwrap();
        let stale_request = query::QueryRequest::new("mutation($id:ID!, $input:SettingsMutationInputArg!) { update_Settings(docID:$id,input:$input) { _docID } }")
            .with_variables(json!({"id":doc_id,"input":stale_patch}));
        let result = defra_http::client_signed_update::execute(&url, &authorization, stale_request, None, &mut stale).await;
        assert!(matches!(result, Err(defra_http::client_signed_create::ClientSignedCreateError::UpdateChallenge(_))), "{result:?}");
        let current = node.execute_request(query::QueryRequest::new("query { Settings { name } }").with_identity(Some(identity.did().unwrap()))).await;
        assert!(!current.has_errors(), "{:?}", current.errors);
        assert_eq!(current.data.unwrap()["Settings"][0]["name"], "after");

        let history_query = defra_http::client_signed_update::history_request(&doc_id);
        let latest: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&token)
            .json(&json!({"query":history_query.query, "variables":history_query.variables})).send().await.unwrap().json().await.unwrap();
        let latest = defra_http::client_signed_update::snapshot_from_response(&doc_id, &latest).unwrap();
        let mut transactions = Vec::new();
        for _ in 0..2 {
            let value: Value = http.post(format!("http://{address}/api/v0/tx")).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
            transactions.push(value["id"].as_u64().unwrap().to_string());
        }
        for (txn, patch) in transactions.iter().zip([json!({"name":"disjoint"}), json!({"extra":"first"})]) {
            let mut graph = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &latest, &signing).unwrap();
            let request = query::QueryRequest::new("mutation($id:ID!, $input:SettingsMutationInputArg!) { update_Settings(docID:$id,input:$input) { _docID } }")
                .with_variables(json!({"id":doc_id,"input":patch}));
            let response = defra_http::client_signed_update::execute(&url, &authorization, request, Some(txn), &mut graph).await.unwrap();
            assert!(response.errors.is_empty(), "{:?}", response.errors);
        }
        for txn in &transactions {
            http.post(format!("http://{address}/api/v0/tx/{txn}")).bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        }
        let current = node.execute_request(query::QueryRequest::new("{ Settings { name extra } }").with_identity(Some(identity.did().unwrap()))).await;
        assert!(!current.has_errors(), "{:?}", current.errors);
        assert_eq!(current.data.unwrap()["Settings"][0], json!({"name":"disjoint", "extra":"first"}));

        let history_query = defra_http::client_signed_update::history_request(&doc_id);
        let merged: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&token)
            .json(&json!({"query":history_query.query, "variables":history_query.variables})).send().await.unwrap().json().await.unwrap();
        let merged = defra_http::client_signed_update::snapshot_from_response(&doc_id, &merged).unwrap();
        let mut transactions = Vec::new();
        for _ in 0..2 {
            let value: Value = http.post(format!("http://{address}/api/v0/tx")).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
            transactions.push(value["id"].as_u64().unwrap().to_string());
        }
        for (txn, patch) in transactions.iter().zip([json!({"name":"winner"}), json!({"name":"loser"})]) {
            let mut graph = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &merged, &signing).unwrap();
            let request = query::QueryRequest::new("mutation($id:ID!, $input:SettingsMutationInputArg!) { update_Settings(docID:$id,input:$input) { _docID } }")
                .with_variables(json!({"id":doc_id,"input":patch}));
            let response = defra_http::client_signed_update::execute(&url, &authorization, request, Some(txn), &mut graph).await.unwrap();
            assert!(response.errors.is_empty(), "{:?}", response.errors);
        }
        http.post(format!("http://{address}/api/v0/tx/{}", transactions[0])).bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        let conflict = http.post(format!("http://{address}/api/v0/tx/{}", transactions[1])).bearer_auth(&token).send().await.unwrap();
        assert_eq!(conflict.status().as_u16(), 409);
        let current = node.execute_request(query::QueryRequest::new("{ Settings { name extra } }").with_identity(Some(identity.did().unwrap()))).await;
        assert!(!current.has_errors(), "{:?}", current.errors);
        assert_eq!(current.data.unwrap()["Settings"][0], json!({"name":"winner", "extra":"first"}));

        let history_query = defra_http::client_signed_update::history_request(&doc_id);
        let latest: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&token)
            .json(&json!({"query":history_query.query, "variables":history_query.variables})).send().await.unwrap().json().await.unwrap();
        let latest = defra_http::client_signed_update::snapshot_from_response(&doc_id, &latest).unwrap();
        let mut transactions = Vec::new();
        for _ in 0..2 {
            let value: Value = http.post(format!("http://{address}/api/v0/tx")).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
            transactions.push(value["id"].as_u64().unwrap().to_string());
        }
        let conditional_patch = json!({"extra":"conditional"});
        let mut conditional = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &conditional_patch, &latest, &signing).unwrap();
        let conditional_request = query::QueryRequest::new(r#"mutation($id:ID!, $input:SettingsMutationInputArg!) { update_Settings(filter:{_docID:{_eq:$id}, name:{_eq:"winner"}},input:$input) { _docID } }"#)
            .with_variables(json!({"id":doc_id,"input":conditional_patch}));
        let response = defra_http::client_signed_update::execute(&url, &authorization, conditional_request, Some(&transactions[0]), &mut conditional).await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let competing_patch = json!({"name":"changed"});
        let mut competing = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &competing_patch, &latest, &signing).unwrap();
        let request = query::QueryRequest::new("mutation($id:ID!, $input:SettingsMutationInputArg!) { update_Settings(docID:$id,input:$input) { _docID } }")
            .with_variables(json!({"id":doc_id,"input":competing_patch}));
        let response = defra_http::client_signed_update::execute(&url, &authorization, request, Some(&transactions[1]), &mut competing).await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        http.post(format!("http://{address}/api/v0/tx/{}", transactions[1])).bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        let conflict = http.post(format!("http://{address}/api/v0/tx/{}", transactions[0])).bearer_auth(&token).send().await.unwrap();
        assert_eq!(conflict.status().as_u16(), 409, "predicate field changed before conditional commit");
        let current = node.execute_request(query::QueryRequest::new("{ Settings { name extra } }").with_identity(Some(identity.did().unwrap()))).await;
        assert_eq!(current.data.unwrap()["Settings"][0], json!({"name":"changed", "extra":"first"}));

        let history_query = defra_http::client_signed_update::history_request(&doc_id);
        let current: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&token)
            .json(&json!({"query":history_query.query, "variables":history_query.variables})).send().await.unwrap().json().await.unwrap();
        let current = defra_http::client_signed_update::snapshot_from_response(&doc_id, &current).unwrap();
        let patch = json!({"extra":"accepted"});
        let request = |predicate: &str| query::QueryRequest::new(format!(r#"mutation($id:ID!, $input:SettingsMutationInputArg!) {{ update_Settings(filter:{{_docID:{{_eq:$id}}, name:{{_eq:"{predicate}"}}}},input:$input) {{ _docID _version {{ cid }} }} }}"#))
            .with_variables(json!({"id":doc_id,"input":patch}));
        let mut denied = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &current, &signing).unwrap();
        let denied = defra_http::client_signed_update::execute(&url, &authorization, request("winner"), None, &mut denied).await;
        assert!(matches!(denied, Err(defra_http::client_signed_create::ClientSignedCreateError::Incomplete)), "{denied:?}");
        let mut graph = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &current, &signing).unwrap();
        let response = defra_http::client_signed_update::execute(&url, &authorization, request("changed"), None, &mut graph).await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let cid = response.data.unwrap()["update_Settings"][0]["_version"][0]["cid"].as_str().unwrap().to_owned();
        assert_eq!(node.verified_block_signer_did(&cid).await.unwrap(), caller);
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn http_client_signed_update_rechecks_acp_and_cancellation() {
    use async_tungstenite::tungstenite::Message;
    use crypto::{Key as _, PrivateKey as _};
    use db::block::builder::{ClientCreateSignatures, ClientUpdateSignatures};
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use futures::StreamExt;
    use identity::Identity as _;

    const POLICY: &str = r#"
name: Client update
resources:
  - name: settings
    relations:
      - name: reader
        types: [actor]
      - name: editor
        types: [actor]
    permissions:
      - name: read
        expr: reader + editor
      - name: update
        expr: editor
      - name: delete
"#;
    edge::testing::edge().block_on(async {
        let owner_private = crypto::generate_ed25519().unwrap();
        let owner_public = owner_private.public_key();
        let owner_signing = SigningConfig { key_type: SigningKeyType::Ed25519, private_key_bytes: owner_private.raw_owned(), public_key_bytes: owner_public.raw_owned(), public_key_hex: hex::encode(owner_public.raw()), remote_signer: None, signing_authorization: None };
        let owner = identity::RawIdentity::from_private_key(owner_private).unwrap();
        let owner_did = owner.did().unwrap().to_string();
        let editor_private = crypto::generate_ed25519().unwrap();
        let editor_public = editor_private.public_key();
        let editor_signing = SigningConfig { key_type: SigningKeyType::Ed25519, private_key_bytes: editor_private.raw_owned(), public_key_bytes: editor_public.raw_owned(), public_key_hex: hex::encode(editor_public.raw()), remote_signer: None, signing_authorization: None };
        let editor = identity::RawIdentity::from_private_key(editor_private).unwrap();
        let editor_did = editor.did().unwrap().to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().enable_signing().with_node_acp_owner(owner_did.clone())
            .with_http(HttpConfig::with_listener(listener).unwrap()).build().await.unwrap();
        let owner_context = defra_core::RequestContext::default().with_identity(Some(owner_did.clone()));
        let policy = node.add_dac_policy_as(&owner_context, POLICY).await.unwrap();
        node.add_collection(&format!("type ProtectedSettings @policy(id: \"{policy}\", resource: \"settings\") {{ name: String }}")).await.unwrap();
        let schema = node.get_collection("ProtectedSettings").unwrap().unwrap();
        for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::DocumentUpdate, acp::nac::NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&owner_context, permission.as_str(), &editor_did).await.unwrap();
        }
        let owner_token = String::from_utf8(identity::new_token(&owner, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let editor_token = String::from_utf8(identity::new_token(&editor, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let url = format!("ws://{address}/api/v0/graphql/client-sign");
        let initial = json!({"name":"before"});
        let mut create = ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &initial, &owner_signing).unwrap();
        let created = defra_http::client_signed_create::execute(&url, &format!("Bearer {owner_token}"),
            query::QueryRequest::new("mutation($input:ProtectedSettingsMutationInputArg!) { add_ProtectedSettings(input:$input) { _docID } }").with_variables(json!({"input":initial})), None, &mut create).await.unwrap();
        assert!(created.errors.is_empty(), "{:?}", created.errors);
        let doc_id = created.data.unwrap()["add_ProtectedSettings"][0]["_docID"].as_str().unwrap().to_owned();
        let query = defra_http::client_signed_update::history_request(&doc_id);
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        let history: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&owner_token)
            .json(&json!({"query":query.query,"variables":query.variables})).send().await.unwrap().json().await.unwrap();
        let snapshot = defra_http::client_signed_update::snapshot_from_response(&doc_id, &history).unwrap();
        node.add_dac_actor_relationship_as(&owner_context, "ProtectedSettings", &doc_id, "reader", &editor_did).await.unwrap();
        let patch = json!({"name":"after"});
        let request = || query::QueryRequest::new("mutation($id:ID!, $input:ProtectedSettingsMutationInputArg!) { update_ProtectedSettings(docID:$id,input:$input) { _docID _version { cid } } }")
            .with_variables(json!({"id":doc_id,"input":patch}));
        let graph = || ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &snapshot, &editor_signing).unwrap();
        let mut denied_graph = graph();
        let denied = defra_http::client_signed_update::execute(&url, &format!("Bearer {editor_token}"), request(), None, &mut denied_graph).await.unwrap();
        assert!(!denied.errors.is_empty(), "{denied:?}");

        node.add_dac_actor_relationship_as(&owner_context, "ProtectedSettings", &doc_id, "editor", &editor_did).await.unwrap();
        let handshake = defra_http::client_signed_create::websocket_request(&url, &format!("Bearer {editor_token}")).unwrap();
        let stream = net::connect(handshake.uri()).await.unwrap();
        let (mut socket, _) = async_tungstenite::client_async_with_config(handshake, stream, Some(defra_http::client_signed_create::websocket_config())).await.unwrap();
        let operation = request();
        socket.send(Message::Text(json!({"query":operation.query,"variables":operation.variables,"operationName":operation.operation_name,"txn_id":null,"public_key_hex":editor_signing.public_key_hex}).to_string().into())).await.unwrap();
        let frame = edge::testing::edge().timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        let Message::Text(frame) = frame else { panic!("expected signing wave") };
        let wave: Value = serde_json::from_str(&frame).unwrap();
        let blocks = wave["blocks"].as_array().unwrap().iter().map(|value| hex::decode(value.as_str().unwrap()).unwrap()).collect::<Vec<_>>();
        let mut revoked_graph = graph();
        let signatures = revoked_graph.respond(&blocks).unwrap().iter().map(hex::encode).collect::<Vec<_>>();
        node.delete_dac_actor_relationship_as(&owner_context, "ProtectedSettings", &doc_id, "editor", &editor_did).await.unwrap();
        socket.send(Message::Text(json!({"signatures":signatures}).to_string().into())).await.unwrap();
        let frame = edge::testing::edge().timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        let Message::Text(frame) = frame else { panic!("expected denied response") };
        let response: Value = serde_json::from_str(&frame).unwrap();
        assert!(!response["response"]["errors"].as_array().unwrap().is_empty(), "{response}");

        node.add_dac_actor_relationship_as(&owner_context, "ProtectedSettings", &doc_id, "editor", &editor_did).await.unwrap();
        let handshake = defra_http::client_signed_create::websocket_request(&url, &format!("Bearer {editor_token}")).unwrap();
        let stream = net::connect(handshake.uri()).await.unwrap();
        let (mut socket, _) = async_tungstenite::client_async_with_config(handshake, stream, Some(defra_http::client_signed_create::websocket_config())).await.unwrap();
        let operation = request();
        socket.send(Message::Text(json!({"query":operation.query,"variables":operation.variables,"operationName":operation.operation_name,"txn_id":null,"public_key_hex":editor_signing.public_key_hex}).to_string().into())).await.unwrap();
        let frame = edge::testing::edge().timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        assert!(matches!(frame, Message::Text(_)));
        drop(socket);
        let read = node.execute_request(query::QueryRequest::new("{ ProtectedSettings { name } }").with_identity(Some(owner.did().unwrap()))).await;
        assert!(!read.has_errors(), "{:?}", read.errors);
        assert_eq!(read.data.unwrap()["ProtectedSettings"][0]["name"], "before");
        let mut allowed_graph = graph();
        let allowed = defra_http::client_signed_update::execute(&url, &format!("Bearer {editor_token}"), request(), None, &mut allowed_graph).await.unwrap();
        assert!(allowed.errors.is_empty(), "{:?}", allowed.errors);
        let cid = allowed.data.unwrap()["update_ProtectedSettings"][0]["_version"][0]["cid"].as_str().unwrap().to_owned();
        let (block_bytes, signature_bytes) = node.authorized_signed_block_bytes(&cid, Some(&editor_did)).await.unwrap();
        let mut block = defra_core::block::Block::from_dag_cbor(&block_bytes).unwrap();
        let signature = defra_core::block::Signature::from_dag_cbor(&signature_bytes).unwrap();
        assert_eq!(block.signature.take(), Some(signature.generate_cid().unwrap()));
        let public = crypto::public_key_from_string(crypto::KeyType::Ed25519, &editor_signing.public_key_hex).unwrap();
        assert_eq!(public.did().unwrap(), editor_did);
        assert!(public.verify(&block.to_dag_cbor().unwrap(), &signature.value).unwrap());

        let query = defra_http::client_signed_update::history_request(&doc_id);
        let history: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&owner_token)
            .json(&json!({"query":query.query,"variables":query.variables})).send().await.unwrap().json().await.unwrap();
        let latest = defra_http::client_signed_update::snapshot_from_response(&doc_id, &history).unwrap();
        let patch = json!({"name":"unknown-outcome"});
        let mut uncertain = ClientUpdateSignatures::from_input(&storage::Crdts::default(), &schema, &doc_id, &patch, &latest, &editor_signing).unwrap();
        let handshake = defra_http::client_signed_create::websocket_request(&url, &format!("Bearer {editor_token}")).unwrap();
        let stream = net::connect(handshake.uri()).await.unwrap();
        let (mut socket, _) = async_tungstenite::client_async_with_config(handshake, stream, Some(defra_http::client_signed_create::websocket_config())).await.unwrap();
        let operation = query::QueryRequest::new("mutation($id:ID!, $input:ProtectedSettingsMutationInputArg!) { update_ProtectedSettings(docID:$id,input:$input) { _docID } }")
            .with_variables(json!({"id":doc_id,"input":patch}));
        socket.send(Message::Text(json!({"query":operation.query,"variables":operation.variables,"operationName":operation.operation_name,"txn_id":null,"public_key_hex":editor_signing.public_key_hex}).to_string().into())).await.unwrap();
        let frame = edge::testing::edge().timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        let Message::Text(frame) = frame else { panic!("expected signing wave") };
        let wave: Value = serde_json::from_str(&frame).unwrap();
        let blocks = wave["blocks"].as_array().unwrap().iter().map(|value| hex::decode(value.as_str().unwrap()).unwrap()).collect::<Vec<_>>();
        let signatures = uncertain.respond(&blocks).unwrap().iter().map(hex::encode).collect::<Vec<_>>();
        socket.send(Message::Text(json!({"signatures":signatures}).to_string().into())).await.unwrap();
        drop(socket);
        assert!(uncertain.is_complete());
        assert!(uncertain.respond(&blocks).is_err());
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn client_signed_delete_obeys_owner_visibility_and_transaction_boundaries() {
    use async_tungstenite::tungstenite::Message;
    use crypto::{Key as _, PrivateKey as _};
    use db::block::builder::{ClientCreateSignatures, ClientDeleteSignatures};
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use futures::StreamExt;
    use identity::Identity as _;

    const POLICY: &str = r#"
name: Signed delete
resources:
  - name: records
    relations:
      - name: reader
        types: [actor]
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;
    edge::testing::edge().block_on(async {
        let make_identity = || {
            let private = crypto::generate_ed25519().unwrap();
            let public = private.public_key();
            let signing = SigningConfig { key_type: SigningKeyType::Ed25519,
                private_key_bytes: private.raw_owned(), public_key_bytes: public.raw_owned(),
                public_key_hex: hex::encode(public.raw()), remote_signer: None, signing_authorization: None };
            (identity::RawIdentity::from_private_key(private).unwrap(), signing)
        };
        let (owner, owner_signing) = make_identity();
        let (other, other_signing) = make_identity();
        let owner_did = owner.did().unwrap().to_string();
        let other_did = other.did().unwrap().to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().enable_signing().with_node_acp_owner(owner_did.clone())
            .with_http(HttpConfig::with_listener(listener).unwrap()).build().await.unwrap();
        let context = defra_core::RequestContext::default().with_identity(Some(owner_did.clone()));
        let policy = node.add_dac_policy_as(&context, POLICY).await.unwrap();
        node.add_collection(&format!("type DeleteRecord @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let schema = node.get_collection("DeleteRecord").unwrap().unwrap();
        for permission in [acp::nac::NodePermission::DocumentRead, acp::nac::NodePermission::DocumentDelete, acp::nac::NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&context, permission.as_str(), &other_did).await.unwrap();
        }
        let token = |identity: &identity::RawIdentity| String::from_utf8(identity::new_token(identity, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let owner_token = token(&owner);
        let other_token = token(&other);
        let auth = |token: &str| format!("Bearer {token}");
        let ws = format!("ws://{address}/api/v0/graphql/client-sign");
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        let input = json!({"name":"owned"});
        let mut create = ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &input, &owner_signing).unwrap();
        let created = defra_http::client_signed_create::execute(&ws, &auth(&owner_token),
            query::QueryRequest::new("mutation($input: DeleteRecordMutationInputArg!) { create_DeleteRecord(input:$input) {_docID} }")
                .with_variables(json!({"input":input})), None, &mut create).await.unwrap();
        assert!(created.errors.is_empty(), "{:?}", created.errors);
        let doc_id = created.data.unwrap()["add_DeleteRecord"][0]["_docID"].as_str().unwrap().to_owned();
        let history = defra_http::client_signed_update::history_request(&doc_id);
        let query_history = |token: &str| http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(token)
            .json(&json!({"query":history.query,"variables":history.variables}));
        let hidden: query::QueryResponse = query_history(&other_token).send().await.unwrap().json().await.unwrap();
        assert!(defra_http::client_signed_update::snapshot_from_response(&doc_id, &hidden).is_err());
        let visible: query::QueryResponse = query_history(&owner_token).send().await.unwrap().json().await.unwrap();
        let snapshot = defra_http::client_signed_update::snapshot_from_response(&doc_id, &visible).unwrap();
        let delete = || query::QueryRequest::new("mutation($id: ID!) { delete_DeleteRecord(docIDs: [$id]) { _docID } }").with_variables(json!({"id":doc_id}));
        let make_graph = |signing: &SigningConfig| ClientDeleteSignatures::from_snapshot(&schema, &doc_id, &snapshot, signing).unwrap();
        let mut hidden_graph = make_graph(&other_signing);
        let hidden_result = defra_http::client_signed_delete::execute(&ws, &auth(&other_token), delete(), None, &mut hidden_graph).await;
        assert!(hidden_result.is_err() || hidden_result.unwrap().has_errors());
        node.add_dac_actor_relationship_as(&context, "DeleteRecord", &doc_id, "reader", &other_did).await.unwrap();
        let mut denied_graph = make_graph(&other_signing);
        let denied = defra_http::client_signed_delete::execute(&ws, &auth(&other_token), delete(), None, &mut denied_graph).await;
        assert!(denied.is_err() || denied.unwrap().has_errors());
        let new_tx = |token: &str| http.post(format!("http://{address}/api/v0/tx")).bearer_auth(token);
        let other_tx: Value = new_tx(&other_token).send().await.unwrap().json().await.unwrap();
        let other_tx = other_tx["id"].as_u64().unwrap().to_string();
        let mut mismatch = make_graph(&owner_signing);
        let result = defra_http::client_signed_delete::execute(&ws, &auth(&owner_token), delete(), Some(&other_tx), &mut mismatch).await;
        assert!(result.is_err() || result.unwrap().has_errors());
        http.delete(format!("http://{address}/api/v0/tx/{other_tx}")).bearer_auth(&other_token).send().await.unwrap().error_for_status().unwrap();
        let owner_tx: Value = new_tx(&owner_token).send().await.unwrap().json().await.unwrap();
        let owner_tx = owner_tx["id"].as_u64().unwrap().to_string();
        let mut rolled_back = make_graph(&owner_signing);
        let deleted = defra_http::client_signed_delete::execute(&ws, &auth(&owner_token), delete(), Some(&owner_tx), &mut rolled_back).await.unwrap();
        assert!(!deleted.has_errors());
        http.delete(format!("http://{address}/api/v0/tx/{owner_tx}")).bearer_auth(&owner_token).send().await.unwrap().error_for_status().unwrap();
        let retained = node.execute_request(query::QueryRequest::new("{ DeleteRecord { _docID } }").with_identity(Some(owner.did().unwrap()))).await;
        assert_eq!(retained.data.unwrap()["DeleteRecord"][0]["_docID"], doc_id);
        let mut final_graph = make_graph(&owner_signing);
        let mut block = defra_core::block::Block::from_dag_cbor(final_graph.composite_block()).unwrap();
        let (signature_cid, _) = db::block::builder::compute_signature(&block, &owner_signing).unwrap().unwrap();
        block.signature = Some(signature_cid);
        let deleted_cid = block.generate_cid().unwrap().to_string();
        let result = defra_http::client_signed_delete::execute(&ws, &auth(&owner_token), delete(), None, &mut final_graph).await.unwrap();
        assert!(!result.has_errors());
        assert_eq!(result.data.unwrap()["delete_DeleteRecord"][0]["_docID"], doc_id);
        assert_eq!(node.verified_block_signer_did(&deleted_cid).await.unwrap(), owner_did);
        let retained = node.execute_request(query::QueryRequest::new("{ DeleteRecord { _docID } }").with_identity(Some(owner.did().unwrap()))).await;
        assert!(retained.data.unwrap()["DeleteRecord"].as_array().unwrap().is_empty());
        let input = json!({"name":"unknown"});
        let mut create = ClientCreateSignatures::from_input(&storage::Crdts::default(), &schema, &input, &owner_signing).unwrap();
        let created = defra_http::client_signed_create::execute(&ws, &auth(&owner_token),
            query::QueryRequest::new("mutation($input: DeleteRecordMutationInputArg!) { create_DeleteRecord(input:$input) {_docID} }")
                .with_variables(json!({"input":input})), None, &mut create).await.unwrap();
        let unknown_id = created.data.unwrap()["add_DeleteRecord"][0]["_docID"].as_str().unwrap().to_owned();
        let query = defra_http::client_signed_update::history_request(&unknown_id);
        let history: query::QueryResponse = http.post(format!("http://{address}/api/v0/graphql")).bearer_auth(&owner_token)
            .json(&json!({"query":query.query,"variables":query.variables})).send().await.unwrap().json().await.unwrap();
        let snapshot = defra_http::client_signed_update::snapshot_from_response(&unknown_id, &history).unwrap();
        let mut unknown = ClientDeleteSignatures::from_snapshot(&schema, &unknown_id, &snapshot, &owner_signing).unwrap();
        let handshake = defra_http::client_signed_create::websocket_request(&ws, &auth(&owner_token)).unwrap();
        let stream = net::connect(handshake.uri()).await.unwrap();
        let (mut socket, _) = async_tungstenite::client_async_with_config(handshake, stream, Some(defra_http::client_signed_create::websocket_config())).await.unwrap();
        let operation = query::QueryRequest::new("mutation($id: ID!) { delete_DeleteRecord(docIDs: [$id]) { _docID } }").with_variables(json!({"id":unknown_id}));
        socket.send(Message::Text(json!({"query":operation.query,"variables":operation.variables,"operationName":operation.operation_name,"txn_id":null,"public_key_hex":owner_signing.public_key_hex}).to_string().into())).await.unwrap();
        let frame = edge::testing::edge().timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        let Message::Text(frame) = frame else { panic!("expected signing wave") };
        let wave: Value = serde_json::from_str(&frame).unwrap();
        let blocks = wave["blocks"].as_array().unwrap().iter().map(|value| hex::decode(value.as_str().unwrap()).unwrap()).collect::<Vec<_>>();
        let signatures = unknown.respond(&blocks).unwrap().iter().map(hex::encode).collect::<Vec<_>>();
        socket.send(Message::Text(json!({"signatures":signatures}).to_string().into())).await.unwrap();
        drop(socket);
        assert!(unknown.is_complete());
        assert!(unknown.respond(&blocks).is_err());
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn http_client_signs_large_document_in_an_explicit_transaction() {
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
        node.add_collection("type LargeRecord { contents: String }").await.unwrap();
        let schema = node.get_collection("LargeRecord").unwrap().unwrap();
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
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        let txn: Value = http.post(format!("http://{address}/api/v0/tx")).bearer_auth(&token)
            .send().await.unwrap().json().await.unwrap();
        let txn = txn["id"].as_u64().unwrap().to_string();
        let mut doc = Document::new(schema);
        let text = "session history ✓".repeat(120_000);
        assert!(text.len() > 1024 * 1024);
        doc.set("contents", NormalValue::String(text.clone())).unwrap();
        let request = query::QueryRequest::new("mutation($input: LargeRecordMutationInputArg!) { create_LargeRecord(input:$input) { _docID _version { cid } } }")
            .with_variables(json!({"input": doc.to_map().unwrap()}));
        let mut graph = db::block::builder::ClientCreateSignatures::new(&storage::Crdts::default(), &doc, &signing).unwrap();
        let response = defra_http::client_signed_create::execute(
            &format!("ws://{address}/api/v0/graphql/client-sign"), &format!("Bearer {token}"), request, Some(&txn), &mut graph,
        ).await.unwrap();
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let data = response.data.unwrap();
        let cid = data["add_LargeRecord"][0]["_version"][0]["cid"].as_str().unwrap();
        assert!(node.verified_block_signer_did(cid).await.is_err());
        http.post(format!("http://{address}/api/v0/tx/{txn}"))
            .bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        assert_eq!(node.verified_block_signer_did(cid).await.unwrap(), caller);
        let response: Value = http.post(format!("http://{address}/api/v0/graphql"))
            .bearer_auth(&token).json(&json!({"query":"{ LargeRecord { contents } }"}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(response["data"]["LargeRecord"][0]["contents"], text);
        node.shutdown().await.unwrap();
    }).unwrap();
}

#[test]
fn incomplete_fragmented_signing_request_can_be_discarded_without_a_write() {
    use async_tungstenite::tungstenite::{
        client::IntoClientRequest,
        protocol::frame::{
            coding::{Data, OpCode},
            Frame,
        },
        Message,
    };
    use crypto::{Key as _, PrivateKey as _};
    use futures::SinkExt;

    edge::testing::edge().block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let node = Node::builder().enable_signing()
            .with_http(HttpConfig::with_listener(listener).unwrap()).build().await.unwrap();
        node.add_collection("type LargeRecord { contents: String }").await.unwrap();
        let private = crypto::generate_ed25519().unwrap();
        let public_key_hex = hex::encode(private.public_key().raw());
        let identity = identity::RawIdentity::from_private_key(private).unwrap();
        let token = String::from_utf8(identity::new_token(&identity, Duration::from_secs(60), Some(address.to_string()), None).unwrap()).unwrap();
        let http = net::http::Client::builder(edge::testing::edge()).no_proxy().build().unwrap();
        let txn: Value = http.post(format!("http://{address}/api/v0/tx")).bearer_auth(&token)
            .send().await.unwrap().json().await.unwrap();
        let txn = txn["id"].as_u64().unwrap().to_string();
        let mut handshake = format!("ws://{address}/api/v0/graphql/client-sign").into_client_request().unwrap();
        handshake.headers_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
        let stream = net::connect(handshake.uri()).await.unwrap();
        let (mut socket, _) = async_tungstenite::client_async(handshake, stream).await.unwrap();
        let start = json!({"query":"mutation($input: LargeRecordMutationInputArg!) { create_LargeRecord(input:$input) { _docID } }", "variables":{"input":{"contents":"test"}}, "txn_id":txn, "public_key_hex":public_key_hex}).to_string();
        socket.send(Message::Frame(Frame::message(start.into_bytes(), OpCode::Data(Data::Text), false))).await.unwrap();
        socket.close(None).await.unwrap();
        http.delete(format!("http://{address}/api/v0/tx/{txn}"))
            .bearer_auth(&token).send().await.unwrap().error_for_status().unwrap();
        let response: Value = http.post(format!("http://{address}/api/v0/graphql"))
            .bearer_auth(&token).json(&json!({"query":"{ LargeRecord { contents } }"}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(response["data"]["LargeRecord"], json!([]));
        node.shutdown().await.unwrap();
    }).unwrap();
}
