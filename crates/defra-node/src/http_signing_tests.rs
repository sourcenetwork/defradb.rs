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
