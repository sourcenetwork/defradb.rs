//! Under Vera, a peer that received a parent and its chunks shows a chunk only
//! to readers of the parent.

use std::time::Duration;

use integration_test::{generate_identity, poll_until, USER_ACP_POLICY};

use super::chunk_support::{body, visible_hashes};
use super::helpers;

const COLLECTIONS: [&str; 2] = ["Article", "Article__body_chunk"];

fn schema(policy_id: &str) -> String {
    format!(
        r#"type Article @policy(id: "{policy_id}", resource: "users") {{
            title: String
            body: String @chunk(maxChars: 64)
        }}"#
    )
}

#[nuclei::test]
#[serial_test::serial]
async fn a_peer_shows_chunks_only_to_readers_of_the_parent() {
    edge::compat(async {
        let jack = helpers::funded_identity();
        let bob = generate_identity(&helpers::defra_binary()).expect("Bob identity");
        let (jack_key, bob_key) = (jack.private_key_hex.as_str(), bob.private_key_hex.as_str());

        let hub = helpers::start_hub_cluster().await;
        let cluster =
            helpers::build_defra_with_vera_rs(&hub.node(0).rpc_url(), jack_key, 2, true).await;
        let (node0, node1) = (cluster.client(0), cluster.client(1));

        let policy = node0.acp_policy_add(USER_ACP_POLICY, jack_key).unwrap();
        let policy_id = policy["PolicyID"]
            .as_str()
            .or_else(|| policy["policyID"].as_str())
            .unwrap()
            .to_string();
        node0
            .schema_add_with_identity(&schema(&policy_id), jack_key)
            .unwrap();
        node1
            .schema_add_with_identity(&schema(&policy_id), jack_key)
            .unwrap();

        let info = node1.p2p_info().unwrap();
        let addr = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .expect("node1 P2P address")
            .to_string();
        node0.p2p_connect(&[&addr]).unwrap();
        node0.p2p_collection_add(&COLLECTIONS).unwrap();
        node1.p2p_collection_add(&COLLECTIONS).unwrap();
        node0
            .p2p_replicator_set_with_identity(&COLLECTIONS, &addr, jack_key)
            .unwrap();

        let data = node0
            .query_with_identity(
                &format!(
                    r#"mutation {{ add_Article(input: {{title: "plan", body: "{}"}}) {{ _docID }} }}"#,
                    body(5, "plans")
                ),
                jack_key,
            )
            .unwrap();
        let doc_id = data["add_Article"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();
        let expected = visible_hashes(&node0, Some(jack_key));
        assert!(!expected.is_empty());

        poll_until(
            || visible_hashes(&node1, Some(jack_key)) == expected,
            Duration::from_secs(60),
            Duration::from_secs(1),
            "the owner sees the replicated chunks on the peer",
        )
        .await;
        assert!(visible_hashes(&node1, Some(bob_key)).is_empty());
        assert!(visible_hashes(&node1, None).is_empty());

        node0
            .acp_relationship_add("Article", &doc_id, "reader", &bob.did, jack_key)
            .unwrap();
        poll_until(
            || visible_hashes(&node1, Some(bob_key)) == expected,
            Duration::from_secs(60),
            Duration::from_secs(1),
            "a reader granted on the parent sees its chunks on the peer",
        )
        .await;
    })
    .await;
}
