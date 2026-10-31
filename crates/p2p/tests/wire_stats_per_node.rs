#![cfg(feature = "test-utils")]

//! Two libp2p nodes in one process keep their own Shinzo wire counts: what
//! one node fetches never shows in the other's `p2p stats`.

use std::time::Duration;

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use p2p::testutil::MockBitswapStore;
use p2p::{P2PHost, P2PHostConfig, PeerId};

fn shinzo() -> P2PHostConfig {
    P2PHostConfig {
        shinzo_wire: true,
        ..Default::default()
    }
}

async fn wait_until_connected(handle: &p2p::P2PHostHandle, peer_id: PeerId) {
    let start = std::time::Instant::now();
    while !handle
        .connected_peers()
        .await
        .unwrap_or_default()
        .contains(&peer_id)
    {
        assert!(start.elapsed() < Duration::from_secs(5), "never connected");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn count(fields: &[(&'static str, u64)], name: &str) -> u64 {
    fields
        .iter()
        .find(|(field, _)| *field == name)
        .map_or(0, |(_, n)| *n)
}

#[tokio::test]
async fn two_nodes_in_one_process_count_their_own_car_traffic() {
    let (asker, asker_handle, _e0, _r0) = P2PHost::with_config(MockBitswapStore::new(), shinzo())
        .await
        .unwrap();
    let (server, server_handle, _e1, _r1) = P2PHost::with_config(MockBitswapStore::new(), shinzo())
        .await
        .unwrap();
    tokio::spawn(asker.run());
    tokio::spawn(server.run());

    server_handle
        .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .await
        .unwrap();
    let address = server_handle.listen_addresses().await.unwrap().remove(0);
    let server_id = server_handle.local_peer_id_cached();
    asker_handle.dial(server_id, vec![address]).await.unwrap();
    wait_until_connected(&asker_handle, server_id).await;

    // A head the server does not hold: it answers with an empty entry, and
    // the asker counts a missed fetch.
    let head = Cid::new_v1(0x71, Code::Sha2_256.digest(b"not held"));
    asker_handle
        .send_car_request(server_id, head)
        .await
        .unwrap();

    let (mut asked, mut served) = (Vec::new(), Vec::new());
    let start = std::time::Instant::now();
    while count(&asked, "carFetchMissed") == 0 {
        assert!(start.elapsed() < Duration::from_secs(10), "no CAR reply");
        tokio::time::sleep(Duration::from_millis(50)).await;
        asked.extend(asker_handle.wire_stats().drain_shinzo());
        served.extend(server_handle.wire_stats().drain_shinzo());
    }
    served.extend(server_handle.wire_stats().drain_shinzo());

    assert_eq!(count(&asked, "carFetchMissed"), 1);
    assert_eq!(
        count(&served, "carFetchMissed"),
        0,
        "the server asked for nothing"
    );
    assert_eq!(count(&asked, "carBuilt"), 0, "the asker served nothing");
    assert!(
        !served.is_empty(),
        "the server counted the request it answered, in its own stats"
    );
}
