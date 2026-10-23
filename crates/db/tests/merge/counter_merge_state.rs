//! A counter block is applied at most once, gated on the blockstore's
//! merged-block state. A store that cannot read that state must fail the
//! merge, never read it as "not merged" and apply the block again.

use async_trait::async_trait;
use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use defra_core::block::{Block, CounterDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeErrorDisposition, MergeHandler};
use document::DocID;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::RegolithStore;

/// A blockstore whose merged-block state cannot be read.
struct MergeStateUnreadable(DefraBlockstore<RegolithStore>);

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Blockstore for MergeStateUnreadable {
    async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
        self.0.get(cid).await
    }

    async fn put(&self, cid: &Cid, data: &[u8]) -> blockstore::Result<()> {
        self.0.put(cid, data).await
    }

    async fn put_many(&self, blocks: &[(&Cid, &[u8])]) -> blockstore::Result<()> {
        self.0.put_many(blocks).await
    }

    async fn has(&self, cid: &Cid) -> blockstore::Result<bool> {
        self.0.has(cid).await
    }

    async fn delete(&self, cid: &Cid) -> blockstore::Result<()> {
        self.0.delete(cid).await
    }

    async fn get_size(&self, cid: &Cid) -> blockstore::Result<Option<usize>> {
        self.0.get_size(cid).await
    }

    async fn all_cids(&self) -> blockstore::Result<Vec<Cid>> {
        self.0.all_cids().await
    }

    fn hash_on_read(&self, enabled: bool) {
        self.0.hash_on_read(enabled);
    }

    async fn is_merged(&self, _cid: &Cid) -> blockstore::Result<bool> {
        Err(blockstore::Error::Internal(
            "merged-block state unreadable".to_string(),
        ))
    }

    async fn mark_as_merged(&self, cid: &Cid) -> blockstore::Result<()> {
        self.0.mark_as_merged(cid).await
    }

    async fn mark_batch_as_merged(&self, cids: &[Cid]) -> blockstore::Result<()> {
        self.0.mark_batch_as_merged(cids).await
    }

    async fn get_unmerged(&self) -> blockstore::Result<Vec<Cid>> {
        self.0.get_unmerged().await
    }
}

#[test]
fn unreadable_merge_state_fails_the_counter_merge() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let database = Arc::new(DB::from_arc(store.clone()).unwrap());
        database
            .create_collection(CollectionVersion::new(
                "Counters",
                "v1",
                "col-counters",
                vec![
                    FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                    FieldDescription::new("2", "score", FieldKind::int())
                        .with_crdt_type(CType::PnCounter),
                ],
            ))
            .await
            .unwrap();
        let blocks = Arc::new(MergeStateUnreadable(DefraBlockstore::new(store, true)));
        let handler = DbMergeHandler::new(database.clone(), blocks.clone());

        let mut data = Vec::new();
        ciborium::into_writer(&5_i64, &mut data).unwrap();
        let payload = CounterDeltaPayload {
            field_name: "score".to_string(),
            collection_version_id: "v1".to_string(),
            priority: 1,
            data,
            nonce: 31,
        };
        let block = Block {
            delta: CrdtDelta::Counter(payload.clone()),
            heads: None,
            links: None,
            encryption: None,
            signature: None,
        };
        let cid = block.generate_cid().unwrap();
        blocks
            .put(&cid, &block.to_dag_cbor().unwrap())
            .await
            .unwrap();

        let doc_id = DocID::new_v0(
            "bafyreihgg6a5auqhikq4nvw6fj3kbreovdbazlisbs5kerkahoqwwiz75i"
                .parse()
                .unwrap(),
        )
        .to_string();
        let short_id = database
            .get_collection("Counters")
            .unwrap()
            .unwrap()
            .resolved_root_id();
        let txn = database.new_txn(false).await.unwrap();
        {
            let systemstore = txn.systemstore().unwrap();
            database
                .resolve_or_allocate_doc_short_id(&systemstore, short_id, &doc_id)
                .await
                .unwrap();
            db::docid::map::set_block_doc_id_mapping(&systemstore, &cid.to_string(), &doc_id)
                .await
                .unwrap();
        }
        txn.force_commit().await.unwrap();

        let metadata = BlockMetadata::normal(&doc_id, "col-counters", "did:key:z6Mkr", None, false);
        let error = handler
            .process_counter_delta(&cid, &payload, &metadata)
            .await
            .expect_err("an unreadable merged-block state must fail the merge");
        assert!(matches!(error, MergeError::Storage(_)), "{error:?}");
        assert_eq!(
            handler.error_disposition(&error),
            MergeErrorDisposition::Retryable
        );
    })
    .unwrap()
}
