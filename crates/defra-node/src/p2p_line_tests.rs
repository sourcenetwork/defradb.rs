//! An update too large for gossip reaches the same peers a small one would,
//! two hops away included (#2820).
//!
//! Three iroh nodes in a line, A - B - C, with A and C barred from each other.
//! A node takes a gossip head only from an origin it is connected to, so C
//! hears of A's writes only through B re-announcing them on merge
//! (`rebroadcast_on_merge`). A write to every field of a 160-field document
//! has a head block over gossip's budget, so each hop sends it as a direct
//! PushLog push.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

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

async fn node(rebroadcast_on_merge: bool) -> Node {
    let node = Node::builder()
        .with_p2p(config(rebroadcast_on_merge))
        .build()
        .await
        .expect("build node");
    node.add_collection(&sdl()).await.expect("add collection");
    node.p2p()
        .expect("p2p")
        .add_collections(vec!["Wide".to_string()])
        .await
        .expect("subscribe to Wide");
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
        edge::testing::edge()
            .sleep(Duration::from_millis(100))
            .await;
    }
}

async fn connected(node: &Node) -> Vec<String> {
    node.p2p()
        .expect("p2p")
        .connected_peers()
        .await
        .expect("connected peers")
}

/// Waits until `node`'s Wide document holds `base` plus each field's number.
async fn wait_for_fields(node: &Node, base: usize) {
    let (first, last) = (field(0), field(FIELDS - 1));
    let query = format!("query {{ Wide {{ {first} {last} }} }}");
    let want = serde_json::json!([{ first.clone(): base, last.clone(): base + FIELDS - 1 }]);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = node.execute(&query).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let docs = response.data.as_ref().and_then(|data| data.get("Wide"));
        if docs == Some(&want) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "C never merged the update; last answer {docs:?}"
        );
        edge::testing::edge()
            .sleep(Duration::from_millis(200))
            .await;
    }
}

async fn on_a(node: &Node, request: String) {
    let response = node.execute(&request).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[test]
fn an_update_too_large_for_gossip_reaches_a_peer_two_hops_away() {
    edge::testing::edge()
        .block_on(async {
            let a = node(false).await;
            let b = node(true).await;
            let c = node(false).await;
            let (id_a, id_c) = (peer_id(&a).await, peer_id(&c).await);
            a.deny_p2p_peer(&id_c).await.expect("bar C from A");
            c.deny_p2p_peer(&id_a).await.expect("bar A from C");

            let addr_b = listen_addr(&b).await;
            for node in [&a, &c] {
                node.p2p()
                    .expect("p2p")
                    .connect_peer(&addr_b)
                    .await
                    .expect("connect to B");
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while connected(&b).await.len() < 2 {
                assert!(Instant::now() < deadline, "B never held both peers");
                edge::testing::edge()
                    .sleep(Duration::from_millis(100))
                    .await;
            }
            // Let gossip join the Wide topic through B.
            edge::testing::edge().sleep(Duration::from_secs(2)).await;

            on_a(
                &a,
                format!(
                    "mutation {{ add_Wide(input: [{}]) {{ _docID }} }}",
                    input(0)
                ),
            )
            .await;
            wait_for_fields(&c, 0).await;
            on_a(
                &a,
                format!(
                    "mutation {{ update_Wide(input: {}) {{ _docID }} }}",
                    input(1000)
                ),
            )
            .await;
            wait_for_fields(&c, 1000).await;

            assert!(
                !connected(&c).await.contains(&id_a),
                "A and C must stay apart for this to cross two hops"
            );
            for node in [a, b, c] {
                node.shutdown().await.unwrap();
            }
        })
        .unwrap()
}
