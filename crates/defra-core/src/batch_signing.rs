//! Batch CID collection and Merkle root computation for batch signing.
//!
//! During block creation, individual CIDs are collected into a session, the
//! one a request's [`RequestContext`](crate::RequestContext) names. After all
//! documents in a batch are created, the collected CIDs are used to compute a
//! Merkle root which is then signed as a single batch.

use regolith::sync::OnceCell;
use std::sync::Arc;

use cid::Cid;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use sha2::{Digest, Sha256};

#[cfg(any(test, feature = "ambient"))]
pub use crate::ambient::{get_batch_session_key, set_batch_session_key};

// ---------------------------------------------------------------------------
// Merkle root computation
// ---------------------------------------------------------------------------

/// Compute a SHA256 Merkle root over a set of CIDs, as Go `shinzo-develop`'s
/// `coreblock.ComputeMerkleRoot` does, so a batch signed by either runtime
/// verifies on the other.
///
/// 1. Sort CIDs by their binary encoding.
/// 2. SHA256-hash each CID's binary encoding to produce leaf hashes.
/// 3. Binary tree reduction: pair adjacent hashes → SHA256(left || right).
///    A layer's odd last hash is paired with itself.
///
/// An empty set's root is SHA256 of nothing.
pub fn compute_merkle_root(cids: &[Cid]) -> [u8; 32] {
    let mut sorted: Vec<Vec<u8>> = cids.iter().map(Cid::to_bytes).collect();
    sorted.sort();

    let mut hashes: Vec<[u8; 32]> = sorted
        .iter()
        .map(|bytes| Sha256::digest(bytes).into())
        .collect();
    if hashes.is_empty() {
        return Sha256::digest([]).into();
    }

    while hashes.len() > 1 {
        hashes = hashes
            .chunks(2)
            .map(|pair| {
                let right = pair.get(1).unwrap_or(&pair[0]);
                let mut hasher = Sha256::new();
                hasher.update(pair[0]);
                hasher.update(right);
                hasher.finalize().into()
            })
            .collect();
    }

    hashes[0]
}

// ---------------------------------------------------------------------------
// Global batch collector  (same pattern as IDENTITY_STORE in signing.rs)
// ---------------------------------------------------------------------------

static BATCH_COLLECTORS: OnceCell<HopscotchMap<String, Arc<SegQueue<Cid>>, RandomState>> =
    OnceCell::new();

fn collectors() -> &'static HopscotchMap<String, Arc<SegQueue<Cid>>, RandomState> {
    BATCH_COLLECTORS.get_or_init_racy(|| HopscotchMap::with_hasher(RandomState::default()))
}

/// Start (or reset) a batch collection session for `session_key`.
pub fn batch_start(session_key: &str) {
    collectors().insert(session_key.to_string(), Arc::new(SegQueue::new()));
}

/// Append a CID to the session identified by `session_key`.
/// No-op if the session does not exist.
pub fn batch_collect_cid(session_key: &str, cid: Cid) {
    if let Some(queue) = collectors().get(session_key) {
        queue.push(cid);
    }
}

/// Remove and return all collected CIDs for `session_key`.
pub fn batch_take_cids(session_key: &str) -> Option<Vec<Cid>> {
    let queue = collectors().remove(session_key)?;
    let mut cids = Vec::with_capacity(queue.len());
    while let Some(cid) = queue.pop() {
        cids.push(cid);
    }
    Some(cids)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DAG_CBOR_CODEC;
    use multihash_codetable::{Code, MultihashDigest};

    fn make_cid(data: &[u8]) -> Cid {
        let hash = Code::Sha2_256.digest(data);
        Cid::new_v1(DAG_CBOR_CODEC, hash)
    }

    /// Roots from Go shinzo-develop's `ComputeMerkleRoot` over CIDs of
    /// "doc0".."doc{n-1}" (CIDv1, dag-cbor, sha2-256).
    #[test]
    fn merkle_root_matches_go() {
        let go = [
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                1,
                "782ce04bf0186cce2029bccb3fb6d13f2ec6bb54903a5ab83424be2641fd1799",
            ),
            (
                2,
                "d0858ea2895874438d6973fd2a78e32314e67a00ff1ad84389048c42c239116b",
            ),
            (
                3,
                "0ffb57f4c557c42494a782e71b2d234ec526603ac639bee54ad50a326377777b",
            ),
            (
                5,
                "5511b03b712ef4d9362cea930743c2d424589b8f8c8e700442ad3f3f6d4e9637",
            ),
        ];
        for (count, root) in go {
            let cids: Vec<Cid> = (0..count)
                .map(|i| make_cid(format!("doc{i}").as_bytes()))
                .collect();
            let hex: String = compute_merkle_root(&cids)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(hex, root, "{count} CIDs");
        }
    }

    #[test]
    fn merkle_root_single() {
        let cid = make_cid(b"hello");
        let root = compute_merkle_root(&[cid]);
        // Single element: root == SHA256(cid.to_bytes())
        let mut hasher = Sha256::new();
        hasher.update(cid.to_bytes());
        let expected: [u8; 32] = hasher.finalize().into();
        assert_eq!(root, expected);
    }

    #[test]
    fn merkle_root_deterministic() {
        let a = make_cid(b"a");
        let b = make_cid(b"b");
        let root1 = compute_merkle_root(&[a, b]);
        let root2 = compute_merkle_root(&[b, a]);
        assert_eq!(root1, root2, "order should not matter (CIDs are sorted)");
    }

    #[test]
    fn merkle_root_different_inputs() {
        let a = make_cid(b"a");
        let b = make_cid(b"b");
        let c = make_cid(b"c");
        let root_ab = compute_merkle_root(&[a, b]);
        let root_ac = compute_merkle_root(&[a, c]);
        assert_ne!(root_ab, root_ac);
    }

    #[test]
    fn batch_collect_roundtrip() {
        let key = "test-session-roundtrip";
        batch_start(key);
        let c1 = make_cid(b"doc1");
        let c2 = make_cid(b"doc2");
        batch_collect_cid(key, c1);
        batch_collect_cid(key, c2);
        let cids = batch_take_cids(key).unwrap();
        assert_eq!(cids.len(), 2);
        assert!(batch_take_cids(key).is_none(), "session should be consumed");
    }

    #[test]
    fn batch_collect_noop_without_start() {
        let key = "nonexistent-session";
        let c = make_cid(b"stray");
        batch_collect_cid(key, c);
        assert!(batch_take_cids(key).is_none());
    }

    #[test]
    fn thread_local_session_key() {
        set_batch_session_key(Some("my-key".to_string()));
        assert_eq!(get_batch_session_key(), Some("my-key".to_string()));
        set_batch_session_key(None);
        assert_eq!(get_batch_session_key(), None);
    }
}
