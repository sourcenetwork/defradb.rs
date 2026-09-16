//! The replication policy gate allows what the default policy allows, narrows
//! with an app policy, and fails closed when the policy errors.

use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use p2p::bitswap::LateBoundServeAcp;
use p2p::replication_policy::{
    DefaultReplicationPolicy, InboundRequest, OutboundBlock, OutboundPath, PolicyPeer,
    ReplicationPolicy, ReplicationPolicyGate,
};

struct Failing;

#[async_trait]
impl ReplicationPolicy for Failing {
    async fn may_send(
        &self,
        _peer: &PolicyPeer<'_>,
        _path: OutboundPath,
        _block: &OutboundBlock<'_>,
    ) -> Result<bool, String> {
        Err("unavailable".to_string())
    }

    async fn may_accept(
        &self,
        _peer: &PolicyPeer<'_>,
        _request: InboundRequest,
        _collection_id: &str,
    ) -> Result<bool, String> {
        Err("unavailable".to_string())
    }
}

fn block(cid: &Cid) -> OutboundBlock<'_> {
    OutboundBlock {
        cid,
        collection_id: "collection",
        doc_ids: &[],
    }
}

fn cid() -> Cid {
    "bafyreihcr6zapk5cnwe7aatmo3bhw6ho6t6zfi632agqprxakc5tkhwkc4"
        .parse()
        .unwrap()
}

#[tokio::test]
async fn no_policy_and_the_default_policy_allow_everything() {
    let cid = cid();
    for gate in [
        ReplicationPolicyGate::new(Arc::new(LateBoundServeAcp::new())),
        {
            let gate = ReplicationPolicyGate::new(Arc::new(LateBoundServeAcp::new()));
            gate.set(Arc::new(DefaultReplicationPolicy));
            gate
        },
    ] {
        for path in [OutboundPath::Push, OutboundPath::Serve] {
            assert!(gate.may_send("peer", path, &block(&cid)).await);
        }
        for request in [InboundRequest::Push, InboundRequest::SyncRequest] {
            assert!(gate.may_accept("peer", request, "collection").await);
        }
    }
}

#[tokio::test]
async fn a_failing_policy_withholds_and_refuses() {
    let cid = cid();
    let gate = ReplicationPolicyGate::new(Arc::new(LateBoundServeAcp::new()));
    gate.set(Arc::new(Failing));
    for path in [OutboundPath::Push, OutboundPath::Serve] {
        assert!(!gate.may_send("peer", path, &block(&cid)).await);
    }
    for request in [InboundRequest::Push, InboundRequest::SyncRequest] {
        assert!(!gate.may_accept("peer", request, "collection").await);
    }
}
