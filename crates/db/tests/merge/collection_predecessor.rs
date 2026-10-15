use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::{database::DB, merge::merge_handler::DbMergeHandler};
use defra_core::block::{Block, CollectionDefinitionDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeOutcome};
use storage::corekv::Key;
use storage::keys::systemstore::CollectionKey;
use storage::RegolithStore;

#[tokio::test]
async fn a_patch_waits_for_the_full_predecessor_instead_of_rebuilding_it() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let defined = query::parse_sdl(
        r#"type Agent @policy(id: "p1", resource: "agents") @branchable {
            did: String @immutable @index
        }"#,
    )
    .unwrap()
    .remove(0);
    db.create_collection(defined).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    let held = db.get_collection("Agent").unwrap().unwrap();
    let previous: Cid = held.schema().version_id.parse().unwrap();
    assert!(blockstore.has(&previous).await.unwrap());
    let key = CollectionKey::new(previous.to_string()).bytes();
    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let full_definition = systemstore.get(&key).await.unwrap().unwrap();
    let predecessor: schema::CollectionVersion = serde_json::from_slice(&full_definition).unwrap();
    systemstore.delete(&key).await.unwrap();
    drop(systemstore);
    txn.commit().await.unwrap();

    let payload = CollectionDefinitionDeltaPayload::new(2);
    let patch = Block::new(
        CrdtDelta::CollectionDefinition(payload.clone()),
        vec![previous],
        vec![],
    );
    let cid = patch.generate_cid().unwrap();
    let metadata = BlockMetadata::normal("", "", "peer", None, false);
    let outcome = handler
        .process_collection_definition_delta(&cid, &patch, &payload, &metadata)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    assert!(systemstore.get(&key).await.unwrap().is_none());
    assert!(systemstore
        .get(&CollectionKey::new(cid.to_string()).bytes())
        .await
        .unwrap()
        .is_none());
    drop(systemstore);
    txn.discard().unwrap();

    let mut placeholder = predecessor.clone();
    placeholder.is_placeholder = true;
    let txn = db.new_txn(false).await.unwrap();
    txn.systemstore()
        .unwrap()
        .set(&key, &serde_json::to_vec(&placeholder).unwrap())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert!(matches!(
        handler
            .process_collection_definition_delta(&cid, &patch, &payload, &metadata)
            .await
            .unwrap(),
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));

    let txn = db.new_txn(false).await.unwrap();
    txn.systemstore()
        .unwrap()
        .set(&key, &full_definition)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(
        handler
            .process_collection_definition_delta(&cid, &patch, &payload, &metadata)
            .await
            .unwrap(),
        MergeOutcome::Merged
    );
    let txn = db.new_txn(true).await.unwrap();
    let bytes = txn
        .systemstore()
        .unwrap()
        .get(&CollectionKey::new(cid.to_string()).bytes())
        .await
        .unwrap()
        .unwrap();
    let patched: schema::CollectionVersion = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(patched.version_id, cid.to_string());
    assert_eq!(patched.policy, predecessor.policy);
    assert_eq!(patched.indexes, predecessor.indexes);
    assert_eq!(patched.is_branchable, predecessor.is_branchable);
    assert_eq!(patched.fields, predecessor.fields);
    txn.discard().unwrap();
}
