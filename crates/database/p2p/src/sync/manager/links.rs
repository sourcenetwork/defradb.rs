//! IPLD link extraction and missing link detection.

use rapidhash::{HashSetExt, RapidHashSet};
use std::collections::VecDeque;

use bytes::Bytes;
use cid::Cid;

use blockstore::Blockstore;

use crate::error::{Error, Result};
use crate::sync::manager::diagnostics::SyncDiagnostics;

/// Parse IPLD block data and return the referenced CIDs that participate in
/// Block-sync DAG transfer.
///
/// This mirrors Go DefraDB's `Block.AllLinks()` semantics for the purpose of
/// DAG completion: the `encryption` link is deliberately excluded. Go stores
/// encryption-metadata blocks in a separate `Encstore` and serves them ONLY
/// over the KMS `encryption` pubsub topic (ECIES-wrapped), never via block sync —
/// see `internal/db/p2p/sync_dag.go:loadBlockLinks`, which fetches the
/// encryption block via `kms.GetKeys` and then walks only `block.AllLinks()`.
///
/// Including the encryption CID here makes a Rust reader block-sync a
/// block that a Go provider never serves, hanging the DAG fetch forever
/// (issue #976). The encryption block is obtained instead during merge via
/// `decrypt_block_data` (KMS DEK fetch). Signature links are kept, matching
/// Go's `hasAccess`, which serves signature blocks over block sync.
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
    // served over block sync (KMS-only), so requesting it stalls cross-runtime DAG
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
///
/// `Ok([])` means every reachable unmerged block was read. A child whose read
/// fails, or that does not parse, fails the whole walk: the caller does not
/// know the frontier and must not take the DAG as complete
/// ([`frontier_or_unknown`]). A storage conflict stays
/// `Error::BlockstoreTxnConflict`.
pub async fn find_all_missing_links<B>(blockstore: &B, block_data: &[u8]) -> Result<Vec<Cid>>
where
    B: Blockstore,
{
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

            // One read per unmerged child. A block deleted since it was linked
            // reads as None and is missing; a failed read leaves the frontier
            // unknown, so it fails the walk instead of passing as present.
            match blockstore
                .get(&link_cid)
                .await
                .map_err(Error::from_blockstore)?
            {
                Some(child_data) => queue.push_back(child_data),
                None => {
                    tracing::debug!(cid = %link_cid, "Missing block at depth in DAG");
                    missing.push(link_cid);
                }
            }
        }
    }

    Ok(missing)
}

/// The frontier a walk of `root_cid`'s DAG leaves: its missing blocks, or,
/// when the walk failed, `[root_cid]`, which is never empty, so a local read
/// fault never reads as a complete DAG. The fault is counted and traced; the
/// root's owner retries it as it retries any incomplete DAG.
pub(crate) fn frontier_or_unknown(
    root_cid: Cid,
    walk: Result<Vec<Cid>>,
    diagnostics: &SyncDiagnostics,
) -> Vec<Cid> {
    match walk {
        Ok(missing) => missing,
        Err(error) => {
            diagnostics.record_missing_link_walk_error();
            tracing::warn!(
                root_cid = %root_cid,
                error = %error,
                "DAG walk failed; its frontier is unknown, so the root stays incomplete"
            );
            vec![root_cid]
        }
    }
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
    /// `encryption` topic, never over block sync (issue #976). Named field links
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
            "encryption link must be excluded from block-sync DAG walk"
        );
    }
}
