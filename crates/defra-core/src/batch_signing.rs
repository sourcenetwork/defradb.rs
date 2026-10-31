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
///
/// This is [`MerkleScheme::GoShinzo`]: `[a, b, c]` and `[a, b, c, c]` (with
/// `c` last in sorted order) share a root, and a leaf hash can stand for an
/// inner node. A verifier must also check the signed CID count.
pub fn compute_merkle_root(cids: &[Cid]) -> [u8; 32] {
    compute_merkle_root_with(cids, MerkleScheme::GoShinzo)
}

/// How a batch signature's Merkle tree is built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MerkleScheme {
    /// Go `shinzo-develop`'s tree, which today's Go Shinzo nodes verify.
    #[default]
    GoShinzo,
    /// Leaves hashed as `SHA256(0x00 || cid)` and inner nodes as
    /// `SHA256(0x01 || left || right)`; a layer's odd last node moves up
    /// unchanged. No two different CID lists share a root, and no leaf
    /// stands for an inner node. Go Shinzo does not verify it yet.
    DomainSeparated,
    /// Rust's tree before Go Shinzo's became the default: Go's untagged
    /// hashes, but a layer's odd last node moves up unchanged. Signatures
    /// made then carry no scheme, so they read back as `GoShinzo`; a verifier
    /// that still holds them selects this scheme to check them. Nothing
    /// signs with it by default.
    RustLegacy,
}

impl MerkleScheme {
    /// Whether a layer's odd last node moves up unchanged, rather than being
    /// paired with itself as Go does.
    fn promotes_odd(self) -> bool {
        matches!(self, Self::DomainSeparated | Self::RustLegacy)
    }
}

const LEAF_TAG: u8 = 0x00;
const INNER_TAG: u8 = 0x01;

/// Compute a batch signature's Merkle root over `cids` with `scheme`. CIDs
/// are sorted by their binary encoding first, so the order they were
/// collected in does not matter. An empty set's root is SHA256 of nothing.
pub fn compute_merkle_root_with(cids: &[Cid], scheme: MerkleScheme) -> [u8; 32] {
    let mut hashes = leaf_layer(cids, scheme);
    if hashes.is_empty() {
        return Sha256::digest([]).into();
    }
    while hashes.len() > 1 {
        hashes = up(&hashes, scheme);
    }
    hashes[0]
}

fn leaf_hash(cid: &[u8], scheme: MerkleScheme) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if scheme == MerkleScheme::DomainSeparated {
        hasher.update([LEAF_TAG]);
    }
    hasher.update(cid);
    hasher.finalize().into()
}

fn inner_hash(left: &[u8; 32], right: &[u8; 32], scheme: MerkleScheme) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if scheme == MerkleScheme::DomainSeparated {
        hasher.update([INNER_TAG]);
    }
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// The leaf hashes, in the CIDs' sorted byte order.
fn leaf_layer(cids: &[Cid], scheme: MerkleScheme) -> Vec<[u8; 32]> {
    let mut sorted: Vec<Vec<u8>> = cids.iter().map(Cid::to_bytes).collect();
    sorted.sort();
    sorted.iter().map(|cid| leaf_hash(cid, scheme)).collect()
}

/// One layer up: neighbours hashed in pairs; a last hash alone is paired
/// with itself (Go) or moves up unchanged (domain-separated, Rust legacy).
fn up(layer: &[[u8; 32]], scheme: MerkleScheme) -> Vec<[u8; 32]> {
    layer
        .chunks(2)
        .map(|pair| match (scheme, pair) {
            (scheme, [alone]) if scheme.promotes_odd() => *alone,
            (_, [alone]) => inner_hash(alone, alone, scheme),
            _ => inner_hash(&pair[0], &pair[1], scheme),
        })
        .collect()
}

/// Where one CID sits in a batch's Merkle tree: its position in sorted
/// order, how many CIDs the batch has, and the sibling hash at each layer
/// that has one, bottom up. With the batch's signature it shows the CID was
/// signed without sending the whole batch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MerklePath {
    pub index: usize,
    pub count: usize,
    pub siblings: Vec<[u8; 32]>,
}

/// The path from `cid` to the root of the tree over `cids`, or `None` when
/// `cid` is not among them.
pub fn merkle_path(cids: &[Cid], cid: &Cid, scheme: MerkleScheme) -> Option<MerklePath> {
    let mut sorted: Vec<Vec<u8>> = cids.iter().map(Cid::to_bytes).collect();
    sorted.sort();
    let mut index = sorted.binary_search(&cid.to_bytes()).ok()?;
    let mut layer = leaf_layer(cids, scheme);
    let path_index = index;
    let mut siblings = Vec::new();
    while layer.len() > 1 {
        let sibling = index ^ 1;
        if sibling < layer.len() {
            siblings.push(layer[sibling]);
        }
        layer = up(&layer, scheme);
        index /= 2;
    }
    Some(MerklePath {
        index: path_index,
        count: cids.len(),
        siblings,
    })
}

/// The root that `cid` at `path` leads to, or `None` when the path does not
/// fit a tree of `path.count` CIDs: an index past the end, or too few or
/// too many siblings for the layers that tree has.
pub fn root_from_path(cid: &Cid, path: &MerklePath, scheme: MerkleScheme) -> Option<[u8; 32]> {
    if path.index >= path.count {
        return None;
    }
    let mut hash = leaf_hash(&cid.to_bytes(), scheme);
    let (mut index, mut count) = (path.index, path.count);
    let mut siblings = path.siblings.iter();
    while count > 1 {
        hash = match (index % 2, index + 1 < count) {
            (1, _) => inner_hash(siblings.next()?, &hash, scheme),
            (_, true) => inner_hash(&hash, siblings.next()?, scheme),
            _ if scheme.promotes_odd() => hash,
            _ => inner_hash(&hash, &hash, scheme),
        };
        index /= 2;
        count = count.div_ceil(2);
    }
    siblings.next().is_none().then_some(hash)
}

// ---------------------------------------------------------------------------
// Global batch collector  (same pattern as IDENTITY_STORE in signing.rs)
// ---------------------------------------------------------------------------

/// Which written blocks a batch signature covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BatchScope {
    /// Each document's composite block, one CID per document write, as Go
    /// `shinzo-develop` collects them, so Rust and Go roots match.
    #[default]
    Composite,
    /// Every block a write produces: composite, field and branchable
    /// collection blocks, so no block of the batch is left unsigned. Go
    /// Shinzo does not collect these.
    Full,
}

/// What a collected CID names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchCid {
    /// A document's composite block.
    Composite,
    /// A field or branchable collection block.
    Other,
}

impl BatchScope {
    /// Whether a session with this scope collects a CID of `kind`.
    pub fn covers(self, kind: BatchCid) -> bool {
        self == BatchScope::Full || kind == BatchCid::Composite
    }
}

/// How a batch is signed and verified: the Merkle tree and the blocks it
/// covers. The default is Go Shinzo's, so today's Go nodes interoperate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchOptions {
    pub scheme: MerkleScheme,
    pub scope: BatchScope,
}

/// One session's CIDs and the scope it collects.
struct Session {
    scope: BatchScope,
    cids: SegQueue<Cid>,
}

static BATCH_COLLECTORS: OnceCell<HopscotchMap<String, Arc<Session>, RandomState>> =
    OnceCell::new();

fn collectors() -> &'static HopscotchMap<String, Arc<Session>, RandomState> {
    BATCH_COLLECTORS.get_or_init_racy(|| HopscotchMap::with_hasher(RandomState::default()))
}

/// Start (or reset) a batch collection session for `session_key`, covering
/// composite blocks only (Go Shinzo's scope).
pub fn batch_start(session_key: &str) {
    batch_start_with(session_key, BatchScope::default());
}

/// Start (or reset) a batch collection session for `session_key` that
/// collects the blocks `scope` covers.
pub fn batch_start_with(session_key: &str, scope: BatchScope) {
    let session = Session {
        scope,
        cids: SegQueue::new(),
    };
    collectors().insert(session_key.to_string(), Arc::new(session));
}

/// Append a CID of `kind` to the session identified by `session_key`, if
/// its scope covers that kind. No-op if the session does not exist.
pub fn batch_collect_cid(session_key: &str, cid: Cid, kind: BatchCid) {
    if let Some(session) = collectors().get(session_key) {
        if session.scope.covers(kind) {
            session.cids.push(cid);
        }
    }
}

/// Remove and return all collected CIDs for `session_key`.
pub fn batch_take_cids(session_key: &str) -> Option<Vec<Cid>> {
    batch_take(session_key).map(|(_, cids)| cids)
}

/// Remove and return the session's scope and all its collected CIDs.
pub fn batch_take(session_key: &str) -> Option<(BatchScope, Vec<Cid>)> {
    let session = collectors().remove(session_key)?;
    let mut cids = Vec::with_capacity(session.cids.len());
    while let Some(cid) = session.cids.pop() {
        cids.push(cid);
    }
    Some((session.scope, cids))
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
        batch_collect_cid(key, c1, BatchCid::Composite);
        batch_collect_cid(key, c2, BatchCid::Composite);
        let cids = batch_take_cids(key).unwrap();
        assert_eq!(cids.len(), 2);
        assert!(batch_take_cids(key).is_none(), "session should be consumed");
    }

    #[test]
    fn a_session_collects_what_its_scope_covers() {
        let (composite, field) = (make_cid(b"composite"), make_cid(b"field"));
        for (key, scope, expected) in [
            ("scope-composite", BatchScope::Composite, vec![composite]),
            ("scope-full", BatchScope::Full, vec![composite, field]),
        ] {
            batch_start_with(key, scope);
            batch_collect_cid(key, composite, BatchCid::Composite);
            batch_collect_cid(key, field, BatchCid::Other);
            assert_eq!(batch_take(key), Some((scope, expected)));
        }
    }

    #[test]
    fn batch_collect_noop_without_start() {
        let key = "nonexistent-session";
        let c = make_cid(b"stray");
        batch_collect_cid(key, c, BatchCid::Composite);
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
