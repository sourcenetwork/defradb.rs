//! A fresh deployment's first start does not wait for its heartbeat.
//!
//! The Vera node finalizes its first block as it starts, before the node's
//! light client subscribes to its headers, and proposes the next block only
//! for a request or at its heartbeat. The node waits for a verified head
//! before it starts.

use std::time::{Duration, Instant};

use super::node;

/// How long a lone validator waits for a request before it proposes anyway.
const HEARTBEAT: Duration = Duration::from_secs(10);

#[test]
fn a_fresh_deployments_first_start_takes_its_first_block() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let first = node(directory.path()).await;
        let took = started.elapsed();
        let status = first
            .vera_acp()
            .expect("vera.rs document ACP")
            .acp_light_client_status()
            .expect("status");
        assert!(status.height >= 1 && status.connected);
        // Before, the light client's first head was the heartbeat's block.
        // A start slowed by load still stands on the first block: it waited
        // for no other.
        assert!(
            took < HEARTBEAT / 2 || status.height == 1,
            "the first start took {took:?} and stands at height {}, as on the heartbeat's block",
            status.height
        );
        first.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
