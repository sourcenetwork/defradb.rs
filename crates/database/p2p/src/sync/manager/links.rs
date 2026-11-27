//! IPLD link extraction and missing link detection.

use rapidhash::{HashSetExt, RapidHashSet};
use std::collections::VecDeque;

use bytes::Bytes;
use cid::Cid;

use blockstore::Blockstore;

use crate::error::{Error, Result};
use crate::rt::time::Instant;
use crate::sync::manager::diagnostics::SyncDiagnostics;
use crate::sync::manager::walk_stats::WalkEnding;

/// Parse IPLD block data and return the referenced CIDs that participate in
/// Block-sync DAG transfer.
///
/// This mirrors Go DefraDB's `Block.AllLinks()` semantics for the purpose of
/// DAG completion: the `encryption` link is deliberately excluded. Go stores
/// encryption-metadata blocks in a separate `Encstore` and serves them ONLY
/// over the KMS `encryption` pubsub topic (ECIES-wrapped), never via Bitswap —
/// see `internal/db/p2p/sync_dag.go:loadBlockLinks`, which fetches the
/// encryption block via `kms.GetKeys` and then walks only `block.AllLinks()`.
///
/// Including the encryption CID here makes a Rust reader block-sync a
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
    // served over block sync (KMS-only), so requesting it stalls cross-runtime DAG
    // fetches. Non-Block payloads (e.g. Lens) won't decode and are left as-is.
    if let Ok(defra_block) = defra_core::Block::from_dag_cbor(block_data) {
        if let Some(enc_cid) = defra_block.encryption {
            refs.retain(|cid| *cid != enc_cid);
        }
    }

    Ok(refs)
}

/// Children a walk looks at before it gives its core to other tasks, and the
/// grain at which a fetch past its quantum can stop it.
pub(crate) const MISSING_LINK_WALK_PAGE: u64 = 128;

/// How a bounded walk ([`walk_missing_links`]) ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WalkEnd {
    /// Every reachable unmerged block was read; these are missing.
    Frontier(Vec<Cid>),
    /// The deadline passed at a page boundary. Says nothing about the DAG:
    /// never empty, never complete.
    Preempted,
}

/// Iteratively find ALL missing blocks in the DAG rooted at `block_data`.
///
/// Walks the entire DAG using a work queue instead of recursion, so stack
/// depth is constant regardless of DAG depth. Uses `is_merged()` to
/// short-circuit traversal of already-merged subtrees. Yields after every
/// [`MISSING_LINK_WALK_PAGE`] children and records its length, pages and time
/// in `diagnostics`.
///
/// `Ok([])` means every reachable unmerged block was read. A child whose read
/// fails, or that does not parse, fails the whole walk: the caller does not
/// know the frontier and must not take the DAG as complete
/// ([`frontier_or_unknown`]). A storage conflict stays
/// `Error::BlockstoreTxnConflict`.
pub async fn find_all_missing_links<B>(
    blockstore: &B,
    block_data: &[u8],
    diagnostics: &SyncDiagnostics,
) -> Result<Vec<Cid>>
where
    B: Blockstore,
{
    match walk_missing_links(blockstore, block_data, diagnostics, None).await? {
        WalkEnd::Frontier(missing) => Ok(missing),
        WalkEnd::Preempted => unreachable!("a walk without a deadline is never preempted"),
    }
}

/// [`find_all_missing_links`], stopped at the first page boundary at or past
/// `deadline`. A preempted walk keeps nothing: the next walk starts again
/// from the root, so a caller passes a deadline only once it has made other
/// progress, and the walk it repeats can never be its only work.
pub(crate) async fn walk_missing_links<B>(
    blockstore: &B,
    block_data: &[u8],
    diagnostics: &SyncDiagnostics,
    deadline: Option<Instant>,
) -> Result<WalkEnd>
where
    B: Blockstore,
{
    let started = Instant::now();
    let mut visits = 0u64;
    let mut pages = 0u64;
    let walk = walk_pages(blockstore, block_data, deadline, &mut visits, &mut pages).await;
    let ending = match &walk {
        Ok(WalkEnd::Frontier(_)) => WalkEnding::Completed,
        Ok(WalkEnd::Preempted) => WalkEnding::Preempted,
        Err(_) => WalkEnding::Failed,
    };
    diagnostics.record_missing_link_walk(ending, visits, pages, started.elapsed());
    walk
}

async fn walk_pages<B>(
    blockstore: &B,
    block_data: &[u8],
    deadline: Option<Instant>,
    visits: &mut u64,
    pages: &mut u64,
) -> Result<WalkEnd>
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
            *visits += 1;
            if visits.is_multiple_of(MISSING_LINK_WALK_PAGE) {
                if deadline.is_some_and(|end| Instant::now() >= end) {
                    return Ok(WalkEnd::Preempted);
                }
                crate::rt::yield_now().await;
                *pages += 1;
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

    Ok(WalkEnd::Frontier(missing))
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
