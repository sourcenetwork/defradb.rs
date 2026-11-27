use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use storage::RegolithStore;

use super::tests::{create_composite_block, create_lww_block, TestTransport};
use crate::message::{DocSyncItem, DocSyncReply};
use crate::sync::{SyncConfig, SyncCoordinator, SyncEvent};
use crate::transport::PeerId;

fn reply(doc_id: &str, heads: &[cid::Cid]) -> DocSyncReply {
    DocSyncReply {
        results: vec![DocSyncItem {
            doc_id: doc_id.to_string(),
            heads: heads.iter().map(|cid| cid.to_bytes()).collect(),
        }],
        ..DocSyncReply::default()
    }
}

fn already_merged_reports(events: &mut crate::sync::manager::SyncEventReceiver) -> Vec<String> {
    let mut docs = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let SyncEvent::HeadsAlreadyMerged { doc_id, .. } = event {
            docs.push(doc_id);
        }
    }
    docs
}

/// #3493: a reply whose every head of a document is merged here reports the
/// document as already merged, so an explicit sync waiting on it stops
/// waiting instead of running to its timeout.
#[nuclei::test(threads = 0)]
async fn a_reply_with_every_head_merged_reports_the_document() {
    let blockstore = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let (coordinator, mut events) = SyncCoordinator::new(
        TestTransport::new(),
        blockstore.clone(),
        crate::network::test_network(),
        SyncConfig::default(),
    )
    .await
    .expect("coordinator");
    let (field_cid, field_block) = create_lww_block("name");
    let (root_cid, root_block) = create_composite_block("doc1", "name", field_cid);
    blockstore.put(&field_cid, &field_block).await.unwrap();
    blockstore.put(&root_cid, &root_block).await.unwrap();
    blockstore.mark_as_merged(&root_cid).await.unwrap();

    coordinator
        .handle_doc_sync_reply(PeerId::new("peer".to_string()), reply("doc1", &[root_cid]))
        .await
        .unwrap();

    assert_eq!(
        already_merged_reports(&mut events),
        vec!["doc1".to_string()]
    );
}

/// A document with one head merged here and another still to fetch is not
/// reported as already merged: the fetched head reports it when it merges.
#[nuclei::test(threads = 0)]
async fn a_reply_with_a_head_still_to_fetch_does_not_report_the_document() {
    let blockstore = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let (coordinator, mut events) = SyncCoordinator::new(
        TestTransport::new(),
        blockstore.clone(),
        crate::network::test_network(),
        SyncConfig::default(),
    )
    .await
    .expect("coordinator");
    let (field_cid, field_block) = create_lww_block("name");
    let (held_cid, held_block) = create_composite_block("doc1", "name", field_cid);
    let (absent_cid, _) = create_composite_block("doc1", "age", field_cid);
    blockstore.put(&field_cid, &field_block).await.unwrap();
    blockstore.put(&held_cid, &held_block).await.unwrap();
    blockstore.mark_as_merged(&held_cid).await.unwrap();

    coordinator
        .handle_doc_sync_reply(
            PeerId::new("peer".to_string()),
            reply("doc1", &[held_cid, absent_cid]),
        )
        .await
        .unwrap();

    assert!(already_merged_reports(&mut events).is_empty());
}
