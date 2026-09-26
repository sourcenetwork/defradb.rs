use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use db::{database::DB, merge::merge_handler::DbMergeHandler};
use defra_core::block::{
    Block, CompositeDeltaPayload, CounterDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

pub type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

pub async fn make_handler() -> Handler {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    make_handler_on(store).await
}

pub async fn make_handler_on(store: Arc<RegolithStore>) -> Handler {
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(CollectionVersion::new(
        "Users",
        "v1",
        "col-users",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
            FieldDescription::new("3", "score", FieldKind::int()).with_crdt_type(CType::PnCounter),
        ],
    ))
    .await
    .unwrap();
    DbMergeHandler::new(db, Arc::new(DefraBlockstore::new(store, true)))
}

#[derive(Clone, Default)]
pub struct History {
    root: Option<MergeBlock>,
    field_parent: Option<Cid>,
    counter_parent: Option<Cid>,
    revisions: u64,
}

impl History {
    pub fn root(&self) -> &MergeBlock {
        self.root.as_ref().unwrap()
    }

    pub async fn append(&mut self, store: &impl Blockstore, count: u64, prefix: &str) {
        let mut pending = Vec::new();
        for priority in self.revisions + 1..=self.revisions + count {
            let mut data = Vec::new();
            ciborium::into_writer(
                &NormalValue::String(format!("{prefix}-{priority}")),
                &mut data,
            )
            .unwrap();
            let field = Block::new(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".into(),
                    schema_version_id: "v1".into(),
                    priority,
                    data,
                }),
                self.field_parent.into_iter().collect(),
                vec![],
            );
            let field_cid = field.generate_cid().unwrap();
            pending.push((field_cid, field.to_dag_cbor().unwrap()));
            let mut data = Vec::new();
            ciborium::into_writer(&1_i64, &mut data).unwrap();
            let counter = Block::new(
                CrdtDelta::Counter(CounterDeltaPayload {
                    field_name: "score".into(),
                    schema_version_id: "v1".into(),
                    priority,
                    data,
                    nonce: priority.try_into().unwrap(),
                }),
                self.counter_parent.into_iter().collect(),
                vec![],
            );
            let counter_cid = counter.generate_cid().unwrap();
            pending.push((counter_cid, counter.to_dag_cbor().unwrap()));
            let composite = Block::new(
                CrdtDelta::Composite(CompositeDeltaPayload {
                    schema_version_id: "v1".into(),
                    priority,
                    status: 1,
                }),
                self.root.iter().map(|root| root.cid).collect(),
                vec![
                    DAGLink::new("name", field_cid),
                    DAGLink::new("score", counter_cid),
                ],
            );
            let cid = composite.generate_cid().unwrap();
            let bytes = composite.to_dag_cbor().unwrap();
            pending.push((cid, bytes.clone()));
            let doc_id = self
                .root
                .as_ref()
                .map(|root| root.doc_id.clone())
                .unwrap_or_else(|| db::block::builder::derive_doc_id(&cid));
            self.root = Some(MergeBlock {
                cid,
                block_data: bytes.into(),
                doc_id,
                collection_id: "col-users".into(),
                creator: "history-peer".into(),
                sender_peer: Some("history-peer".into()),
                is_explicit_replicator: false,
                explicit_replay_authorization: None,
                verified_creator: None,
            });
            self.field_parent = Some(field_cid);
            self.counter_parent = Some(counter_cid);
            if pending.len() >= 256 {
                flush(store, &mut pending).await;
            }
        }
        flush(store, &mut pending).await;
        self.revisions += count;
    }
}

async fn flush(store: &impl Blockstore, pending: &mut Vec<(Cid, Vec<u8>)>) {
    if !pending.is_empty() {
        let entries: Vec<_> = pending
            .iter()
            .map(|(cid, bytes)| (cid, bytes.as_slice()))
            .collect();
        store.put_many(&entries).await.unwrap();
        pending.clear();
    }
}

pub async fn merge_turn(handler: &Handler, root: &MergeBlock, batch: bool) -> MergeOutcome {
    let result = if batch {
        handler
            .handle_block_batch(std::slice::from_ref(root))
            .await
            .remove(0)
    } else {
        handler
            .handle_block(
                &root.cid,
                &root.block_data,
                BlockMetadata::normal(
                    &root.doc_id,
                    &root.collection_id,
                    &root.creator,
                    root.sender_peer.as_deref(),
                    false,
                ),
            )
            .await
    };
    let outcome = result.expect("valid history must not fail");
    assert!(
        !matches!(outcome, MergeOutcome::Rejected { .. }),
        "valid history was rejected: {outcome:?}"
    );
    outcome
}

pub async fn converge(handler: &Handler, root: &MergeBlock, batch: bool) {
    for _ in 0..256 {
        let outcome = merge_turn(handler, root, batch).await;
        if outcome.is_merged() || outcome.is_terminal_skip() {
            let txn = handler.db().new_txn(true).await.unwrap();
            {
                let store = txn.systemstore().unwrap();
                let mut entries = store
                    .iterator(
                        storage::corekv::IterOptions::new()
                            .with_prefix(format!("/merge-history/v1/{}/", root.cid).into_bytes()),
                    )
                    .await
                    .unwrap();
                assert!(
                    entries.next().await.unwrap().is_none(),
                    "completed history leaked continuation state"
                );
                entries.close().await.unwrap();
            }
            txn.force_discard().unwrap();
            return;
        }
    }
    panic!("history made no bounded progress");
}

pub async fn read_document(handler: &Handler, root: &MergeBlock) -> Option<Document> {
    let collection = handler
        .db()
        .find_collection_by_id(&root.collection_id)
        .unwrap()
        .unwrap();
    let txn = handler.db().new_txn(true).await.unwrap();
    let document = collection
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            &DocID::from_string(&root.doc_id).unwrap(),
        )
        .await
        .unwrap();
    txn.force_discard().unwrap();
    document
}
