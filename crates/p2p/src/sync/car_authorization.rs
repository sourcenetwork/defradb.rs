//! Exact rooted CAR authorization, independent of response pagination.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};

// Bound server work below the transport request deadline. Exhausting this
// budget is a retryable request failure, never a truncated authorization set.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) async fn requested_descendants<B: Blockstore>(
    blockstore: &B,
    root: Cid,
    requested: HashSet<Cid>,
) -> Result<HashSet<Cid>> {
    requested_descendants_with_budget(blockstore, root, requested, AUTHORIZATION_TIMEOUT).await
}

async fn requested_descendants_with_budget<B: Blockstore>(
    blockstore: &B,
    root: Cid,
    requested: HashSet<Cid>,
    budget: Duration,
) -> Result<HashSet<Cid>> {
    tokio::time::timeout(budget, async {
        let mut remaining = requested;
        let mut authorized = HashSet::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([root]);
        while !remaining.is_empty() {
            let Some(cid) = queue.pop_front() else {
                break;
            };
            if !visited.insert(cid) {
                continue;
            }
            // In-memory blockstores may never suspend. Yield so the deadline
            // and other bounded serve tasks can make progress on deep graphs.
            if visited.len() % 128 == 0 {
                tokio::task::yield_now().await;
            }
            let Some(data) = blockstore.get(&cid).await.map_err(Error::from_blockstore)? else {
                continue;
            };
            if remaining.remove(&cid) {
                authorized.insert(cid);
            }
            // Preserve the existing KMS boundary: encryption links never
            // confer CAR authority, even under an authorized document root.
            let links = super::manager::links::extract_ipld_links(&data).unwrap_or_default();
            queue.extend(links.into_iter().filter(|child| !visited.contains(child)));
        }
        Ok(authorized)
    })
    .await
    .map_err(|_| Error::ResponseTimeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockstore::DefraBlockstore;
    use ipld_core::{codec::Codec, ipld};
    use multihash_codetable::{Code, MultihashDigest};
    use serde_ipld_dagcbor::codec::DagCborCodec;
    use std::sync::Arc;
    use storage::RegolithStore;

    #[tokio::test]
    async fn root_authority_excludes_unrelated_and_encryption_blocks() {
        use defra_core::{Block, CrdtDelta, DAGLink, LwwDeltaPayload};

        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let mut cids = Vec::new();
        for value in 0..3 {
            let data = DagCborCodec::encode_to_vec(&ipld!({"value": value})).unwrap();
            let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&cid, &data).await.unwrap();
            cids.push(cid);
        }
        let block = Block::new_with_options(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: "secret".to_string(),
                priority: 1,
                schema_version_id: "schema".to_string(),
                data: b"ciphertext".to_vec(),
            }),
            vec![],
            vec![DAGLink::new("secret", cids[0])],
            Some(cids[1]),
            None,
        );
        let data = block.to_dag_cbor().unwrap();
        let root = block.generate_cid().unwrap();
        store.put(&root, &data).await.unwrap();
        let authorized = requested_descendants(&store, root, cids.iter().copied().collect())
            .await
            .unwrap();
        assert_eq!(authorized, HashSet::from([cids[0]]));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_budget_is_retryable_not_partial_authorization() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let data = DagCborCodec::encode_to_vec(&ipld!({"value": 0})).unwrap();
        let leaf = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&leaf, &data).await.unwrap();
        let mut root = leaf;
        for _ in 0..256 {
            let data = DagCborCodec::encode_to_vec(&ipld!({"child": root})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&root, &data).await.unwrap();
        }
        let result = requested_descendants_with_budget(
            &store,
            root,
            HashSet::from([root, leaf]),
            Duration::ZERO,
        )
        .await;
        let error =
            result.expect_err("a partial grant must not look like a complete authorization result");
        assert!(matches!(error, Error::ResponseTimeout));
        assert!(error.is_connection_like());
        let authorized = requested_descendants(&store, root, HashSet::from([leaf]))
            .await
            .unwrap();
        assert_eq!(authorized, HashSet::from([leaf]));
    }
}
