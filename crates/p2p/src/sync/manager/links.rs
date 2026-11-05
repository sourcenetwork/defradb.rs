//! IPLD link extraction and missing link detection.

use rapidhash::{HashSetExt, RapidHashSet};
use std::collections::VecDeque;

use bytes::Bytes;
use cid::Cid;

use blockstore::Blockstore;

use crate::error::{Error, Result};

/// Parse IPLD block data and return the referenced CIDs that participate in
/// Bitswap DAG transfer.
///
/// This mirrors Go DefraDB's `Block.AllLinks()` semantics for the purpose of
/// DAG completion: the `encryption` link is deliberately excluded. Go stores
/// encryption-metadata blocks in a separate `Encstore` and serves them ONLY
/// over the KMS `encryption` pubsub topic (ECIES-wrapped), never via Bitswap —
/// see `internal/db/p2p/sync_dag.go:loadBlockLinks`, which fetches the
/// encryption block via `kms.GetKeys` and then walks only `block.AllLinks()`.
///
/// Including the encryption CID here makes a Rust reader Bitswap-request a
/// block that a Go provider never serves, hanging the DAG fetch forever
/// (issue #976). The encryption block is obtained instead during merge via
/// `decrypt_block_data` (KMS DEK fetch). Signature links are kept, matching
/// Go's `hasAccess`, which serves signature blocks over Bitswap.
pub(crate) fn extract_ipld_links(block_data: &[u8]) -> Result<Vec<Cid>> {
    use ipld_core::codec::Links;
    use serde_ipld_dagcbor::codec::DagCborCodec;

    let mut refs: Vec<Cid> = match DagCborCodec::links(block_data) {
        Ok(links) => links.collect(),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("Unsupported codec") {
                return Ok(Vec::new());
            }
            return Err(Error::BlockParseError {
                reason: format!("Failed to extract references: {}. Block may be corrupt.", e),
            });
        }
    };

    // Drop the encryption-metadata link if this is a DefraDB block. It is never
    // served over Bitswap (KMS-only), so requesting it stalls cross-runtime DAG
    // fetches. Non-Block payloads (e.g. Lens) won't decode and are left as-is.
    if let Ok(defra_block) = defra_core::Block::from_dag_cbor(block_data) {
        if let Some(enc_cid) = defra_block.encryption {
            refs.retain(|cid| *cid != enc_cid);
        }
    }

    Ok(refs)
}

/// Iteratively find ALL missing blocks in the DAG rooted at `block_data`.
///
/// Walks the entire DAG using a work queue instead of recursion, so stack
/// depth is constant regardless of DAG depth. Uses `is_merged()` to
/// short-circuit traversal of already-merged subtrees.
pub async fn find_all_missing_links<B: Blockstore>(
    blockstore: &B,
    block_data: &[u8],
) -> Result<Vec<Cid>> {
    let mut missing = Vec::new();
    let mut visited = RapidHashSet::new();
    let mut queue: VecDeque<Bytes> = VecDeque::new();
    queue.push_back(Bytes::copy_from_slice(block_data));

    while let Some(data) = queue.pop_front() {
        let refs = extract_ipld_links(&data)?;

        for link_cid in refs {
            if !visited.insert(link_cid) {
                continue;
            }

            // Early-exit: merged subtrees are complete, skip traversal
            match blockstore.is_merged(&link_cid).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        cid = %link_cid,
                        error = %e,
                        "Failed to check merge status, will traverse"
                    );
                }
            }

            match blockstore.has(&link_cid).await {
                Ok(true) => {
                    // An unread child cannot establish a complete frontier.
                    match blockstore.get(&link_cid).await {
                        Ok(Some(child_data)) => queue.push_back(child_data),
                        Ok(None) => {
                            return Err(Error::BlockstoreError(format!(
                                "block {link_cid} disappeared between existence check and read"
                            )));
                        }
                        Err(error) => return Err(Error::from_blockstore(error)),
                    }
                }
                Ok(false) => {
                    tracing::debug!(cid = %link_cid, "Missing block at depth in DAG");
                    missing.push(link_cid);
                }
                Err(error) => return Err(Error::from_blockstore(error)),
            }
        }
    }

    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use defra_core::{Block as DefraBlock, CrdtDelta, DAGLink, LwwDeltaPayload};

    fn dummy_cid(seed: u8) -> Cid {
        use multihash_codetable::{Code, MultihashDigest};
        let hash = Code::Sha2_256.digest(&[seed; 4]);
        Cid::new_v1(0x71, hash)
    }

    /// An encrypted block's `encryption` link must NOT be reported as a missing
    /// DAG block, because Go serves encryption-metadata blocks via the KMS
    /// `encryption` topic, never over Bitswap (issue #976). Named field links
    /// are still reported.
    #[test]
    fn extract_ipld_links_excludes_encryption_link() {
        let field_link = dummy_cid(1);
        let enc_cid = dummy_cid(2);

        let block = DefraBlock::new_with_options(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: "secret".to_string(),
                priority: 1,
                collection_version_id: "schema1".to_string(),
                data: b"ciphertext".to_vec(),
            }),
            vec![],
            vec![DAGLink::new("secret", field_link)],
            Some(enc_cid),
            None,
        );
        let bytes = block.to_dag_cbor().expect("encode encrypted block");

        let links = extract_ipld_links(&bytes).expect("extract links");
        assert!(
            links.contains(&field_link),
            "named field link must be retained"
        );
        assert!(
            !links.contains(&enc_cid),
            "encryption link must be excluded from Bitswap DAG walk"
        );
    }
}

#[cfg(test)]
mod local_walk_investigation {
    use super::*;
    use async_trait::async_trait;
    use defra_core::{Block, CrdtDelta, DAGLink, LwwDeltaPayload};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct CountingStore {
        blocks: HashMap<Cid, Bytes>,
        root: Cid,
        all_merged: AtomicBool,
        checks: AtomicUsize,
        has: AtomicUsize,
        gets: AtomicUsize,
    }

    #[async_trait]
    impl Blockstore for CountingStore {
        async fn get(&self, cid: &Cid) -> blockstore::Result<Option<Bytes>> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(self.blocks.get(cid).cloned())
        }
        async fn has(&self, cid: &Cid) -> blockstore::Result<bool> {
            self.has.fetch_add(1, Ordering::SeqCst);
            Ok(self.blocks.contains_key(cid))
        }
        async fn is_merged(&self, cid: &Cid) -> blockstore::Result<bool> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            Ok(self.blocks.contains_key(cid)
                && (self.all_merged.load(Ordering::SeqCst) || *cid == self.root))
        }
        async fn put(&self, _: &Cid, _: &[u8]) -> blockstore::Result<()> {
            unimplemented!()
        }
        async fn put_many(&self, _: &[(&Cid, &[u8])]) -> blockstore::Result<()> {
            unimplemented!()
        }
        async fn delete(&self, _: &Cid) -> blockstore::Result<()> {
            unimplemented!()
        }
        async fn get_size(&self, _: &Cid) -> blockstore::Result<Option<usize>> {
            unimplemented!()
        }
        async fn all_cids(&self) -> blockstore::Result<Vec<Cid>> {
            unimplemented!()
        }
        fn hash_on_read(&self, _: bool) {
            unimplemented!()
        }
        async fn mark_as_merged(&self, _: &Cid) -> blockstore::Result<()> {
            unimplemented!()
        }
        async fn get_unmerged(&self) -> blockstore::Result<Vec<Cid>> {
            unimplemented!()
        }
    }

    /// A caller handing an already-merged root to the local walk does not stop
    /// traversal at that root: the helper is given bytes, not the root CID.
    /// This synthetic store deliberately marks its descendants unmerged.
    #[nuclei::test]
    async fn merged_root_with_unmerged_descendants_walks_entire_local_chain() {
        const CHILDREN: usize = 1024;
        let mut blocks = HashMap::new();
        let mut previous = None;
        for i in 0..=CHILDREN {
            let block = Block::new(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".to_string(),
                    priority: i as u64,
                    collection_version_id: "schema1".to_string(),
                    data: i.to_le_bytes().to_vec(),
                }),
                vec![],
                previous
                    .into_iter()
                    .map(|cid| DAGLink::new("parent", cid))
                    .collect(),
            );
            let cid = block.generate_cid().unwrap();
            blocks.insert(cid, Bytes::from(block.to_dag_cbor().unwrap()));
            previous = Some(cid);
        }
        let root = previous.unwrap();
        let root_data = blocks[&root].clone();
        let store = CountingStore {
            blocks,
            root,
            all_merged: AtomicBool::new(false),
            checks: AtomicUsize::new(0),
            has: AtomicUsize::new(0),
            gets: AtomicUsize::new(0),
        };
        assert!(store.is_merged(&root).await.unwrap());
        store.checks.store(0, Ordering::SeqCst);
        assert!(find_all_missing_links(&store, &root_data)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(store.checks.load(Ordering::SeqCst), CHILDREN);
        assert_eq!(store.has.load(Ordering::SeqCst), CHILDREN);
        assert_eq!(store.gets.load(Ordering::SeqCst), CHILDREN);
        store.all_merged.store(true, Ordering::SeqCst);
        store.checks.store(0, Ordering::SeqCst);
        store.has.store(0, Ordering::SeqCst);
        store.gets.store(0, Ordering::SeqCst);
        assert!(find_all_missing_links(&store, &root_data)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(store.checks.load(Ordering::SeqCst), 1);
        assert_eq!(store.has.load(Ordering::SeqCst), 0);
        assert_eq!(store.gets.load(Ordering::SeqCst), 0);
    }
}
