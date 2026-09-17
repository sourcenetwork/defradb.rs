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

/// Allows only `allowed`; fails on `failing`; records every document asked about.
struct OneDocument {
    allowed: &'static str,
    failing: &'static str,
    asked: std::sync::Mutex<Vec<Vec<String>>>,
}

#[async_trait]
impl ReplicationPolicy for OneDocument {
    async fn may_send(
        &self,
        _peer: &PolicyPeer<'_>,
        _path: OutboundPath,
        block: &OutboundBlock<'_>,
    ) -> Result<bool, String> {
        self.asked.lock().unwrap().push(block.doc_ids.to_vec());
        if block.doc_ids.iter().any(|doc| doc == self.failing) {
            return Err("unavailable".to_string());
        }
        Ok(block.doc_ids.iter().all(|doc| doc == self.allowed))
    }
}

fn shared<'a>(cid: &'a Cid, doc_ids: &'a [String]) -> OutboundBlock<'a> {
    OutboundBlock {
        cid,
        collection_id: "collection",
        doc_ids,
    }
}

#[tokio::test]
async fn a_shared_block_goes_when_any_of_its_documents_may_go() {
    let cid = cid();
    let docs = [
        "failing".to_string(),
        "withheld".to_string(),
        "allowed".to_string(),
    ];
    let policy = Arc::new(OneDocument {
        allowed: "allowed",
        failing: "failing",
        asked: Default::default(),
    });
    let gate = ReplicationPolicyGate::new(Arc::new(LateBoundServeAcp::new()));
    gate.set(policy.clone());

    assert!(
        gate.may_send("peer", OutboundPath::Serve, &shared(&cid, &docs))
            .await
    );
    assert_eq!(
        *policy.asked.lock().unwrap(),
        vec![
            vec!["failing".to_string()],
            vec!["withheld".to_string()],
            vec!["allowed".to_string()],
        ]
    );
}

#[tokio::test]
async fn a_shared_block_is_withheld_when_none_of_its_documents_may_go() {
    let cid = cid();
    let docs = ["failing".to_string(), "withheld".to_string()];
    let gate = ReplicationPolicyGate::new(Arc::new(LateBoundServeAcp::new()));
    gate.set(Arc::new(OneDocument {
        allowed: "allowed",
        failing: "failing",
        asked: Default::default(),
    }));

    assert!(
        !gate
            .may_send("peer", OutboundPath::Serve, &shared(&cid, &docs))
            .await
    );
}
