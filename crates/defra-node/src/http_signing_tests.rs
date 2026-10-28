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
            let client = reqwest::Client::builder()
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
            node.shutdown().await;
        })
        .unwrap();
}
