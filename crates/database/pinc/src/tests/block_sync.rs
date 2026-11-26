//! A block sync waits for a provider's blocks as long as
//! `PincEndpointConfig::block_sync_timeout` says, not a fixed 30 s.

use std::time::{Duration, Instant};

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{connect, loopback, run, seed, TestNode};
use crate::PincEndpointConfig;
use p2p::transport::{FetchFailureCause, P2PTransport, TransportEvent};

#[test]
fn a_block_sync_a_provider_never_answers_times_out_at_the_configured_timeout() {
    run(async {
        let mut config = PincEndpointConfig::new(seed(), loopback());
        config.max_connections = 8;
        config.block_sync_timeout = Duration::from_millis(300);
        let mut requester = TestNode::with_config(config).await;
        // Nothing answers the CAR requests this node's events carry.
        let provider = TestNode::spawn().await;
        connect(&requester.transport, &provider.transport).await;
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(b"a block nobody serves"));
        let started = Instant::now();
        let query = requester
            .transport
            .sync_blocks(root, vec![provider.id.clone()], Vec::new())
            .await
            .unwrap();
        let cause = requester
            .next(|event| match event {
                TransportEvent::BlockSyncComplete {
                    query_id,
                    success: false,
                    cause,
                    ..
                } if query_id == query => Some(cause),
                _ => None,
            })
            .await;
        assert_eq!(cause, Some(FetchFailureCause::TimedOut));
        // Well under the 30 s default, and no sooner than the timeout.
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(300), "{waited:?}");
        assert!(waited < Duration::from_secs(5), "{waited:?}");
        requester.shutdown().await;
        provider.shutdown().await;
    });
}
