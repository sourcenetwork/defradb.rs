//! A single announced head can contain hundreds of historical revisions.

use super::merge_handler_tests::{create_doc_locally, make_handler_with_schema_and_bus};
use blockstore::Blockstore;
use defra_core::merge::{MergeBlock, MergeHandler, MergeOutcome};
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};
use document::{Document, NormalValue};
use events::{Bus, EventName};
use storage::corekv::Store;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_root_history_uses_bounded_commits_not_one_per_revision() {
    let (handler, blockstore, bus) = make_handler_with_schema_and_bus().await;
    let collection = handler
        .db()
        .find_collection_by_id("col-users")
        .unwrap()
        .unwrap();
    let mut doc = Document::new();
    doc.set("name", NormalValue::String("initial".to_string()));
    doc.set_schema_version_id("v1");
    let (doc_id, _, initial) = create_doc_locally(&handler, &collection, &mut doc, "v1").await;
    let mut field_heads = initial.field_cids;
    let mut composite_heads = vec![initial.cid];
    let mut latest = None;
    for priority in 2..=257 {
        let mut data = Vec::new();
        ciborium::into_writer(&NormalValue::String(format!("name-{priority}")), &mut data).unwrap();
        let field = Block::new(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: "name".into(),
                schema_version_id: "v1".into(),
                priority,
                data,
            }),
            field_heads,
            vec![],
        );
        let field_cid = field.generate_cid().unwrap();
        blockstore
            .put(&field_cid, &field.to_dag_cbor().unwrap())
            .await
            .unwrap();
        let composite = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                schema_version_id: "v1".into(),
                priority,
                status: 1,
            }),
            composite_heads,
            vec![DAGLink::new("name", field_cid)],
        );
        let cid = composite.generate_cid().unwrap();
        let data = composite.to_dag_cbor().unwrap();
        blockstore.put(&cid, &data).await.unwrap();
        latest = Some((cid, data));
        field_heads = vec![field_cid];
        composite_heads = vec![cid];
    }
    let (cid, data) = latest.unwrap();
    let mut raw = bus.subscribe(&[EventName::Update]);
    let mut changes = bus.subscribe_document_changes();
    let observation = tokio::spawn(async move {
        tokio::time::timeout(std::time::Duration::from_secs(5), changes.recv())
            .await
            .unwrap()
            .unwrap()
    });
    let stats = handler.db().store().transaction_stats_handle().unwrap();
    let before = stats.snapshot().commits;
    let results = handler
        .handle_block_batch(&[MergeBlock {
            cid,
            block_data: data.into(),
            doc_id: doc_id.to_string(),
            collection_id: "col-users".into(),
            creator: "did:key:z6MkrDeepCompositeReplay".into(),
            sender_peer: None,
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }])
        .await;
    assert!(
        matches!(&results[..], [Ok(MergeOutcome::Merged)]),
        "{results:?}"
    );
    let commits = stats.snapshot().commits - before;
    assert_eq!(
        commits, 2,
        "one document transaction plus one field-marker transaction"
    );
    let batch = observation.await.unwrap();
    assert_eq!(batch.changes.len(), 1);
    assert_eq!(batch.changes[0].doc_id, doc_id.to_string());
    assert_eq!(
        batch.updates, 257,
        "one state notification for the entire committed history"
    );
    assert!(!batch.resync_required);
    let mut raw_count = 0;
    while raw.try_recv().is_ok() {
        raw_count += 1;
    }
    assert_eq!(
        raw_count, 257,
        "revision subscribers still receive every update"
    );
    assert_eq!(raw.dropped_count(), 0);
    let txn = handler.db().new_txn(true).await.unwrap();
    let stored = collection
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            &doc_id,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.get("name"),
        Some(&NormalValue::String("name-257".into()))
    );
    txn.force_discard().unwrap();
}
