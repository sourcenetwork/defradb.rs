//! Replication of a custom field: two nodes with the type converge under
//! concurrent writes. A node without it merges the rest of each document and
//! parks the field's blocks, and once built again with the type it merges
//! them, with no peer sending them again.
#![cfg(feature = "p2p")]

use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::time::{Duration, Instant};

use defra_node::{EmbeddedNode, P2PConfig};
use serde_json::Value;

use super::max_register::MaxRegister;
use super::SDL;

const NOTE_SDL: &str = "type Note { text: String }";
const COLLECTIONS: [&str; 2] = ["Score", "Note"];

fn p2p_config() -> P2PConfig {
    P2PConfig {
        port: 0,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        allowlist: p2p::iroh::IrohAllowlistConfig::AcceptAll,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
        load_persisted_collections: false,
        max_concurrent_dag_fetches: p2p::sync::DEFAULT_MAX_CONCURRENT_DAG_FETCHES,
        max_concurrent_push_tasks: p2p::sync::DEFAULT_MAX_CONCURRENT_PUSH_TASKS,
        max_doc_sync_request_doc_ids: p2p::sync::DEFAULT_MAX_DOC_SYNC_REQUEST_DOC_IDS,
        rate_limit_burst: p2p::sync::DEFAULT_RATE_LIMIT_BURST,
        rate_limit_rate: p2p::sync::DEFAULT_RATE_LIMIT_RATE,
        max_pending_dags: p2p::sync::DEFAULT_MAX_PENDING_DAGS,
        rebroadcast_on_merge: false,
    }
}

async fn node(with_type: bool, path: Option<&Path>) -> EmbeddedNode {
    let builder = EmbeddedNode::builder().with_p2p(p2p_config());
    let builder = match path {
        Some(path) => builder.data_path(path),
        None => builder,
    };
    let builder = if with_type {
        builder.with_crdt(MaxRegister::new())
    } else {
        builder
    };
    let node = builder.build().await.unwrap();
    node.add_collection(SDL).await.unwrap();
    node.add_collection(NOTE_SDL).await.unwrap();
    node.p2p()
        .unwrap()
        .add_collections(collections())
        .await
        .unwrap();
    node
}

fn collections() -> Vec<String> {
    COLLECTIONS.map(str::to_owned).to_vec()
}

async fn sleep() {
    edge::testing::edge()
        .sleep(Duration::from_millis(100))
        .await;
}

async fn address(node: &EmbeddedNode) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(address) = node
            .p2p()
            .unwrap()
            .listen_addresses()
            .await
            .unwrap()
            .first()
        {
            return address.clone();
        }
        assert!(Instant::now() < deadline, "no P2P listen address");
        sleep().await;
    }
}

/// Let `sender` push to the node at `receiver`, which `authorize` lets accept it.
async fn replicate<F, Fut>(sender: &EmbeddedNode, receiver: &str, authorize: F)
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let sender_address = address(sender).await;
    let p2p = sender.p2p().unwrap();
    p2p.connect_peer(receiver).await.unwrap();
    authorize(sender_address).await;
    p2p.add_replicator(
        collections(),
        Some(receiver),
        Default::default(),
        Vec::new(),
        None,
    )
    .await
    .unwrap();
}

async fn authorize(receiver: &EmbeddedNode, sender: String) {
    receiver
        .p2p()
        .unwrap()
        .add_replicator(
            collections(),
            Some(&sender),
            Default::default(),
            Vec::new(),
            None,
        )
        .await
        .unwrap();
}

async fn run(node: &EmbeddedNode, request: &str) -> Value {
    let response = node.execute(request).await;
    assert!(!response.has_errors(), "{request}: {:?}", response.errors);
    response.data.unwrap()
}

/// Wait until `read` of the node's state holds.
async fn eventually<F, Fut>(what: &str, mut read: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    while !read().await {
        assert!(Instant::now() < deadline, "never: {what}");
        sleep().await;
    }
}

async fn best(node: &EmbeddedNode) -> Value {
    run(node, "{ Score { best } }").await["Score"][0]["best"].clone()
}

#[test]
fn a_node_without_the_type_parks_its_blocks_until_it_has_the_type() {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let a = node(true, None).await;
            let b = node(true, None).await;
            let c = node(false, Some(dir.path())).await;

            let b_address = address(&b).await;
            replicate(&a, &b_address, |sender| authorize(&b, sender)).await;
            let a_address = address(&a).await;
            replicate(&b, &a_address, |sender| authorize(&a, sender)).await;
            let c_address = address(&c).await;
            replicate(&a, &c_address, |sender| authorize(&c, sender)).await;

            run(
                &a,
                r#"mutation { add_Score(input: {player: "ann", best: 5}) { _docID } }"#,
            )
            .await;
            eventually("b has the score", || async { best(&b).await == 5 }).await;

            let (left, right) = futures::join!(
                a.execute(r#"mutation { update_Score(input: {best: 9}) { best } }"#),
                b.execute(r#"mutation { update_Score(input: {best: 7}) { best } }"#),
            );
            assert!(!left.has_errors() && !right.has_errors());
            eventually("a and b converge on the greatest write", || async {
                best(&a).await == 9 && best(&b).await == 9
            })
            .await;

            run(
                &a,
                r#"mutation { add_Note(input: {text: "still here"}) { _docID } }"#,
            )
            .await;
            eventually("c has the note", || async {
                run(&c, "{ Note { text } }").await["Note"]
                    == serde_json::json!([{"text": "still here"}])
            })
            .await;
            assert_eq!(
                run(&c, "{ Score { player } }").await["Score"],
                serde_json::json!([{"player": "ann"}]),
                "c merges the fields it has"
            );
            let hidden = c.execute("{ Score { best } }").await;
            assert!(
                format!("{:?}", hidden.errors).contains(r#"Cannot query field \"best\""#),
                "{:?}",
                hidden.errors
            );

            a.shutdown().await;
            b.shutdown().await;
            c.shutdown().await;
            drop(c);
            let c = EmbeddedNode::builder()
                .data_path(dir.path())
                .with_crdt(MaxRegister::new())
                .build()
                .await
                .unwrap();
            // The replay runs after the build returns. Until it is done the
            // field reads as the blocks merged so far give it: unset, then
            // the create's 5, then the update's 9, never anything else.
            let mut seen: Vec<Value> = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let score = run(&c, "{ Score { player best } }").await["Score"][0].clone();
                assert_eq!(score["player"], "ann");
                let best = score["best"].clone();
                assert!(
                    [Value::Null, serde_json::json!(5), serde_json::json!(9)].contains(&best),
                    "{best}"
                );
                if seen.last() != Some(&best) {
                    seen.push(best.clone());
                }
                if best == 9 {
                    break;
                }
                assert!(Instant::now() < deadline, "never: the parked blocks merge");
                sleep().await;
            }
            let rank = |value: &Value| value.as_i64().unwrap_or(0);
            assert!(
                seen.windows(2).all(|pair| rank(&pair[0]) < rank(&pair[1])),
                "{seen:?}"
            );
            c.shutdown().await;
        })
        .unwrap()
}

#[test]
fn a_parked_block_that_cannot_merge_does_not_hold_up_the_others() {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let a = node(true, None).await;
            let c = node(false, Some(dir.path())).await;
            let c_address = address(&c).await;
            replicate(&a, &c_address, |sender| authorize(&c, sender)).await;

            run(
                &a,
                r#"mutation { add_Score(input: {player: "ann", best: 5}) { _docID } }"#,
            )
            .await;
            for best in [9, 12] {
                run(
                    &a,
                    &format!(r#"mutation {{ update_Score(input: {{best: {best}}}) {{ best }} }}"#),
                )
                .await;
            }
            run(
                &a,
                r#"mutation { add_Note(input: {text: "last"}) { _docID } }"#,
            )
            .await;
            eventually("c has every block", || async {
                run(&c, "{ Note { text } }").await["Note"] == serde_json::json!([{"text": "last"}])
            })
            .await;

            a.shutdown().await;
            c.shutdown().await;
            drop(c);
            // This build's register refuses 12, the newest parked block: it
            // fails on its own and the older blocks merge without it.
            let c = EmbeddedNode::builder()
                .data_path(dir.path())
                .with_crdt(MaxRegister::rejecting(12))
                .build()
                .await
                .unwrap();
            eventually("the other parked blocks merge", || async {
                best(&c).await == 9
            })
            .await;
            c.shutdown().await;
        })
        .unwrap()
}
