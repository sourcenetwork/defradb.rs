//! A hub with pinc's relay role (`HubConfig::relay`): its members reach each
//! other through it by peer id alone, with no address and no record of each
//! other. A hub without the role carries topics only.

use super::{hub_with, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::PincEndpointConfig;
use p2p::transport::P2PTransport;

async fn member_of(hub: &str) -> TestNode {
    let mut config = PincEndpointConfig::new(seed(), loopback());
    config.max_connections = 8;
    config.hubs = vec![hub.to_owned()];
    TestNode::with_config(config).await
}

#[test]
fn members_of_a_relaying_hub_reach_each_other_by_id_alone() {
    run(async {
        let hub = hub_with(true).await;
        let a = member_of(&hub).await;
        let b = member_of(&hub).await;
        // b was never told a's address and never met it: only the relay
        // knows where a is, and carries the session until a direct path works.
        b.transport.dial(&a.id, Vec::new()).await.unwrap();
        b.transport
            .poll_until_connected(&a.id, IO_TIMEOUT)
            .await
            .unwrap();
        a.transport
            .poll_until_connected(&b.id, IO_TIMEOUT)
            .await
            .unwrap();
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn members_of_a_hub_without_the_relay_role_do_not_find_each_other_by_id() {
    run(async {
        let hub = hub_with(false).await;
        let a = member_of(&hub).await;
        let b = member_of(&hub).await;
        let dialed = b.transport.dial(&a.id, Vec::new()).await;
        assert!(dialed.is_err(), "a hub that does not relay found a peer");
        a.shutdown().await;
        b.shutdown().await;
    });
}
