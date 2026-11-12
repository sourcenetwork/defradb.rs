//! An update too large for gossip reaches the same peers a small one would,
//! two hops away and document followers included (#2820).
//!
//! Three iroh nodes in a line, A - B - C, with A and C barred from each other.
//! A node takes a gossip head only from an origin it is connected to, so C
//! hears of A's writes only through B re-announcing them on merge
//! (`rebroadcast_on_merge`). A write to every field of a 160-field document
//! has a head block over gossip's budget, so each hop sends it as a direct
//! PushLog push.
//!
//! These run on the edge runtime, as `p2p_tests.rs` does, because each node
//! binds real iroh sockets.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{Node, P2PConfig, PeerKeySource, SyncTuning};

const FIELDS: usize = 160;

fn field(i: usize) -> String {
    format!("field_number_{i:03}")
}

fn sdl() -> String {
    let fields: Vec<String> = (0..FIELDS).map(|i| format!("{}: Int", field(i))).collect();
    format!("type Wide {{ {} }}", fields.join(" "))
}

/// Every field set to its number plus `base`.
fn input(base: usize) -> String {
    let values: Vec<String> = (0..FIELDS)
        .map(|i| format!("{}: {}", field(i), base + i))
        .collect();
    format!("{{{}}}", values.join(", "))
}

fn config(rebroadcast_on_merge: bool) -> P2PConfig {
    P2PConfig {
        port: Some(0),
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        allowlist: p2p::iroh::IrohAllowlistConfig::AcceptAll,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
        load_persisted_collections: false,
        peer_key: PeerKeySource::File,
        sync: SyncTuning::default(),
        rebroadcast_on_merge,
    }
}

/// A node with the Wide collection, subscribed to it when `subscribe`.
async fn node(rebroadcast_on_merge: bool, subscribe: bool) -> Node {
    let node = Node::builder()
        .with_p2p(config(rebroadcast_on_merge))
        .build()
        .await
        .expect("build node");
    node.add_collection(&sdl()).await.expect("add collection");
    if subscribe {
        node.p2p()
            .expect("p2p")
            .add_collections(vec!["Wide".to_string()])
            .await
            .expect("subscribe to Wide");
    }
    node
}

async fn peer_id(node: &Node) -> String {
    node.p2p()
        .expect("p2p")
        .local_peer_id()
        .await
        .expect("peer id")
}

async fn listen_addr(node: &Node) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let addrs = node
            .p2p()
            .expect("p2p")
            .listen_addresses()
            .await
            .expect("listen addresses");
        if let Some(addr) = addrs.first() {
            return addr.clone();
        }
        assert!(Instant::now() < deadline, "no listen address");
        sleep(100).await;
    }
}

async fn connected(node: &Node) -> Vec<String> {
    node.p2p()
        .expect("p2p")
        .connected_peers()
        .await
        .expect("connected peers")
}

async fn sleep(millis: u64) {
    edge::testing::edge()
        .sleep(Duration::from_millis(millis))
        .await;
}

/// Connects `node` to `to` and waits until `to` holds `peers` peers.
async fn connect(node: &Node, to: &Node, peers: usize) {
    node.p2p()
        .expect("p2p")
        .connect_peer(&listen_addr(to).await)
        .await
        .expect("connect");
    let deadline = Instant::now() + Duration::from_secs(10);
    while connected(to).await.len() < peers {
        assert!(Instant::now() < deadline, "the peer never connected");
        sleep(100).await;
    }
}

/// The first and last fields of `node`'s Wide documents.
async fn wide(node: &Node) -> Option<Value> {
    let (first, last) = (field(0), field(FIELDS - 1));
    let response = node
        .execute(&format!("query {{ Wide {{ {first} {last} }} }}"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.and_then(|data| data.get("Wide").cloned())
}

/// The first and last fields once each holds its number plus `base`.
fn holding(base: usize) -> Value {
    let mut doc = serde_json::Map::new();
    doc.insert(field(0), json!(base));
    doc.insert(field(FIELDS - 1), json!(base + FIELDS - 1));
    json!([Value::Object(doc)])
}

/// Waits until `node`'s Wide document holds `base` plus each field's number.
async fn wait_for_fields(name: &str, node: &Node, base: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let docs = wide(node).await;
        if docs.as_ref() == Some(&holding(base)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{name} never merged the update; last answer {docs:?}"
        );
        sleep(200).await;
    }
}

async fn on(node: &Node, request: String) -> Value {
    let response = node.execute(&request).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.expect("data")
}

fn create(base: usize) -> String {
    format!(
        "mutation {{ add_Wide(input: [{}]) {{ _docID }} }}",
        input(base)
    )
}

fn update(base: usize) -> String {
    format!(
        "mutation {{ update_Wide(input: {}) {{ _docID }} }}",
        input(base)
    )
}

/// A - B - C, with A and C barred from each other and B re-announcing what
/// it merges when `relay`.
async fn line(relay: bool) -> [Node; 3] {
    let a = node(false, true).await;
    let b = node(relay, true).await;
    let c = node(false, true).await;
    let (id_a, id_c) = (peer_id(&a).await, peer_id(&c).await);
    a.deny_p2p_peer(&id_c).await.expect("bar C from A");
    c.deny_p2p_peer(&id_a).await.expect("bar A from C");
    connect(&a, &b, 1).await;
    connect(&c, &b, 2).await;
    // Let gossip join the Wide topic through B.
    sleep(2000).await;
    [a, b, c]
}

async fn stop<const N: usize>(nodes: [Node; N]) {
    for node in nodes {
        node.shutdown().await.unwrap();
    }
}

#[test]
fn an_update_too_large_for_gossip_reaches_a_peer_two_hops_away() {
    edge::testing::edge()
        .block_on(async {
            let [a, b, c] = line(true).await;
            on(&a, create(0)).await;
            wait_for_fields("B", &b, 0).await;
            wait_for_fields("C", &c, 0).await;
            on(&a, update(1000)).await;
            wait_for_fields("B", &b, 1000).await;
            wait_for_fields("C", &c, 1000).await;
            assert!(
                !connected(&c).await.contains(&peer_id(&a).await),
                "A and C must stay apart for this to cross two hops"
            );
            stop([a, b, c]).await;
        })
        .unwrap()
}

/// The control for the test above: with B not re-announcing, B still merges
/// A's write and C never hears of it, so C's copy there came from B.
#[test]
fn without_the_middle_node_re_announcing_the_far_peer_never_gets_it() {
    edge::testing::edge()
        .block_on(async {
            let [a, b, c] = line(false).await;
            on(&a, create(0)).await;
            wait_for_fields("B", &b, 0).await;
            sleep(5000).await;
            assert_eq!(wide(&c).await, Some(json!([])), "C got it without B");
            stop([a, b, c]).await;
        })
        .unwrap()
}

/// A peer that follows one document, and does not subscribe to its
/// collection, gets an update to it too large for gossip, as it gets a small
/// one on the document's topic.
#[test]
fn a_document_follower_gets_an_update_too_large_for_gossip() {
    edge::testing::edge()
        .block_on(async {
            let writer = node(false, true).await;
            let follower = node(false, false).await;
            connect(&follower, &writer, 1).await;
            let created = on(&writer, create(0)).await;
            let doc_id = created["add_Wide"][0]["_docID"]
                .as_str()
                .expect("doc id")
                .to_string();
            follower
                .p2p()
                .expect("p2p")
                .add_documents(vec![defra_p2p_adapter::P2pDocumentRequest {
                    collection: String::new(),
                    doc_id,
                }])
                .await
                .expect("follow the document");
            sleep(2000).await;
            on(&writer, update(1000)).await;
            wait_for_fields("the follower", &follower, 1000).await;
            stop([writer, follower]).await;
        })
        .unwrap()
}
