//! A replicated collection block whose head-set write fails must fail its
//! merge, on the standalone path and on the transactional one, and leave the
//! store as it was so a retry installs the head.

use super::*;
use defra_core::block::CollectionDeltaPayload;
use defra_core::merge::MergeOutcome;

struct CollectionBlock {
    cid: Cid,
    block: Block,
    payload: CollectionDeltaPayload,
}

/// A collection block of `Users` that links no document, over `heads`.
async fn collection_block(
    blocks: &DefraBlockstore<RegolithStore>,
    priority: u64,
    heads: Vec<Cid>,
) -> CollectionBlock {
    let payload = CollectionDeltaPayload {
        collection_version_id: "v1".to_string(),
        priority,
    };
    let block = Block::new(CrdtDelta::Collection(payload.clone()), heads, vec![]);
    let bytes = block.to_dag_cbor().unwrap();
    let cid = generate_cid_from_bytes(&bytes).unwrap();
    blocks.put(&cid, &bytes).await.unwrap();
    CollectionBlock {
        cid,
        block,
        payload,
    }
}

fn metadata() -> BlockMetadata<'static> {
    BlockMetadata::normal("", "col-users", "peer-did", None, false)
}

impl Fixture {
    async fn merge_collection_standalone(
        &self,
        block: &CollectionBlock,
    ) -> std::result::Result<MergeOutcome, String> {
        self.handler
            .process_collection_delta(&block.cid, &block.block, &block.payload, &metadata(), 0)
            .await
            .map_err(|e| e.to_string())
    }

    async fn merge_collection(
        &self,
        block: &CollectionBlock,
    ) -> std::result::Result<MergeOutcome, String> {
        let bytes = block.block.to_dag_cbor().unwrap();
        self.handler
            .handle_block(&block.cid, &bytes, metadata())
            .await
            .map_err(|e| e.to_string())
    }

    async fn live_collection_heads(&self) -> Vec<Cid> {
        let database = self.handler.db();
        let short_id = database
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .resolved_root_id();
        let txn = database.new_txn(true).await.unwrap();
        let found = db::block::heads::live_collection_heads(&txn.headstore().unwrap(), short_id)
            .await
            .unwrap();
        txn.discard().unwrap();
        found.live
    }
}

#[test]
fn collection_head_write_failure_aborts_standalone_merge() {
    workers::block_on(async {
        let fixture = fixture().await;
        let block = collection_block(&fixture.blocks, 1, vec![]).await;
        let before = fixture.snapshot().await;
        fixture.fail_at(&[FailPoint::CollectionHead(block.cid)]);

        let result = fixture.merge_collection_standalone(&block).await;
        assert!(
            result.is_err(),
            "a head write failure must fail the merge: {result:?}"
        );
        assert_eq!(fixture.snapshot().await, before);

        fixture.fail_at(&[]);
        fixture.merge_collection_standalone(&block).await.unwrap();
        assert_eq!(fixture.live_collection_heads().await, vec![block.cid]);
    })
    .unwrap()
}

#[test]
fn collection_marker_write_failure_aborts_standalone_merge() {
    workers::block_on(async {
        let fixture = fixture().await;
        let parent = collection_block(&fixture.blocks, 1, vec![]).await;
        fixture.merge_collection_standalone(&parent).await.unwrap();
        let child = collection_block(&fixture.blocks, 2, vec![parent.cid]).await;
        let before = fixture.snapshot().await;
        fixture.fail_at(&[FailPoint::CollectionSupersede(child.cid)]);

        let result = fixture.merge_collection_standalone(&child).await;
        assert!(
            result.is_err(),
            "a marker write failure must fail the merge: {result:?}"
        );
        assert_eq!(fixture.snapshot().await, before);
        assert_eq!(fixture.live_collection_heads().await, vec![parent.cid]);

        fixture.fail_at(&[]);
        fixture.merge_collection_standalone(&child).await.unwrap();
        assert_eq!(fixture.live_collection_heads().await, vec![child.cid]);
    })
    .unwrap()
}

#[test]
fn collection_head_write_failure_aborts_transactional_merge() {
    workers::block_on(async {
        let fixture = fixture().await;
        let block = collection_block(&fixture.blocks, 1, vec![]).await;
        let before = fixture.snapshot().await;
        fixture.fail_at(&[FailPoint::CollectionHead(block.cid)]);

        let result = fixture.merge_collection(&block).await;
        assert!(
            result.is_err(),
            "a head write failure must fail the merge: {result:?}"
        );
        assert_eq!(fixture.snapshot().await, before);

        fixture.fail_at(&[]);
        fixture.merge_collection(&block).await.unwrap();
        assert_eq!(fixture.live_collection_heads().await, vec![block.cid]);
    })
    .unwrap()
}
