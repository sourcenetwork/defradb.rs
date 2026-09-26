//! A collection block names the documents in a branchable collection's
//! verifiable history. Merging one walks its links, then writes the new
//! collection head.
//!
//! The walk tolerates failure: a link the node does not hold is skipped, and a
//! linked composite whose merge returns an error is logged at debug and
//! skipped. Either way the head is written and the outcome is a *terminal*
//! skip, which replication discharges as merged. The document the block named
//! is then never merged and never retried.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CollectionDeltaPayload, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::NormalValue;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::IterOptions;
use storage::RegolithStore;

const COLLECTION_ID: &str = "col-ledger";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

async fn node() -> (
    Arc<DB<RegolithStore>>,
    Handler,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(
        CollectionVersion::new(
            "Ledger",
            COLLECTION_ID,
            COLLECTION_ID,
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "body", FieldKind::string()),
            ],
        )
        .as_branchable(),
    )
    .await
    .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    (db, handler, blockstore)
}

/// A genesis composite setting `body`, with its field block.
fn document(value: &str) -> (Cid, Vec<(Cid, Vec<u8>)>) {
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "body".to_string(),
            priority: 1,
            schema_version_id: COLLECTION_ID.to_string(),
            data: db::block::builder::encode_value_as_cbor(&NormalValue::String(value.to_string()))
                .unwrap(),
        }),
        vec![],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: COLLECTION_ID.to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("body", field_cid)],
    );
    let composite_cid = composite.generate_cid().unwrap();
    (
        composite_cid,
        vec![
            (field_cid, field.to_dag_cbor().unwrap()),
            (composite_cid, composite.to_dag_cbor().unwrap()),
        ],
    )
}

/// A collection block linking `documents`.
fn collection_block(documents: &[Cid]) -> (Cid, Vec<u8>) {
    collection_block_over(documents, &[])
}

/// A collection block linking `documents`, superseding `heads`.
fn collection_block_over(documents: &[Cid], heads: &[Cid]) -> (Cid, Vec<u8>) {
    let block = Block::new(
        CrdtDelta::Collection(CollectionDeltaPayload {
            schema_version_id: COLLECTION_ID.to_string(),
            priority: if heads.is_empty() { 1 } else { 2 },
        }),
        heads.to_vec(),
        documents
            .iter()
            .map(|cid| DAGLink::new("_head", *cid))
            .collect(),
    );
    let cid = block.generate_cid().unwrap();
    (cid, block.to_dag_cbor().unwrap())
}

async fn merge(handler: &Handler, cid: &Cid, bytes: &[u8]) -> MergeOutcome {
    handler
        .handle_block(
            cid,
            bytes,
            BlockMetadata::normal("", COLLECTION_ID, "peer-did", Some("peer"), false),
        )
        .await
        .unwrap()
}

/// The collection's head keys, read through the headstore's own prefix rather
/// than by matching text across every namespace in the store.
async fn collection_heads(db: &Arc<DB<RegolithStore>>) -> Vec<String> {
    let collection = db.get_collection("Ledger").unwrap().unwrap();
    let prefix =
        storage::keys::headstore::HeadstoreColKey::collection_prefix(collection.resolved_root_id());
    let txn = db.new_txn(true).await.unwrap();
    let keys = {
        let headstore = txn.headstore().unwrap();
        let mut iter = headstore
            .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
            .await
            .unwrap();
        let mut keys = Vec::new();
        while let Some(pair) = iter.next().await.unwrap() {
            keys.push(String::from_utf8_lossy(&pair.key).into_owned());
        }
        iter.close().await.unwrap();
        keys
    };
    let _ = txn.discard();
    keys
}

#[tokio::test]
async fn a_collection_block_whose_link_is_not_held_installs_no_head() {
    let (db, handler, blockstore) = node().await;
    // The composite is authored but never delivered, so the link is missing.
    let (composite_cid, _) = document("first");
    let (cid, bytes) = collection_block(&[composite_cid]);
    blockstore.put(&cid, &bytes).await.unwrap();

    let outcome = merge(&handler, &cid, &bytes).await;

    assert!(
        matches!(
            &outcome,
            MergeOutcome::Skipped {
                terminal: false,
                reason
            } if reason.contains("not held")
        ),
        "a block whose named document was never merged must be retryable, not \
         discharged: {outcome:?}"
    );
    assert!(
        collection_heads(&db).await.is_empty(),
        "no head may be installed for a history this node does not hold"
    );
}

#[tokio::test]
async fn a_collection_block_whose_links_are_held_merges_and_installs_its_head() {
    let (db, handler, blockstore) = node().await;
    let (composite_cid, blocks) = document("first");
    for (block_cid, block_bytes) in &blocks {
        blockstore.put(block_cid, block_bytes).await.unwrap();
    }
    let (cid, bytes) = collection_block(&[composite_cid]);
    blockstore.put(&cid, &bytes).await.unwrap();

    let outcome = merge(&handler, &cid, &bytes).await;

    assert_eq!(outcome, MergeOutcome::Merged);
    let heads = collection_heads(&db).await;
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert!(heads[0].contains(&cid.to_string()), "{heads:?}");
}

/// A partial DAG is an ordinary condition on the receive path: a CAR is
/// truncated at its block and byte caps and the receiver is handed whatever
/// fitted. A historical collection block missing a link must therefore not
/// abort the root's merge, or deep catch-up over a large branchable collection
/// never progresses.
#[tokio::test]
async fn an_ancestor_missing_a_link_does_not_abort_the_root() {
    let (db, handler, blockstore) = node().await;

    // The ancestor names a document that never arrived.
    let (absent, _) = document("historical");
    let (ancestor_cid, ancestor_bytes) = collection_block(&[absent]);
    blockstore
        .put(&ancestor_cid, &ancestor_bytes)
        .await
        .unwrap();

    // The root holds everything it names.
    let (present, blocks) = document("current");
    for (cid, bytes) in &blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    let (root_cid, root_bytes) = collection_block_over(&[present], &[ancestor_cid]);
    blockstore.put(&root_cid, &root_bytes).await.unwrap();

    let outcome = merge(&handler, &root_cid, &root_bytes).await;

    assert_eq!(
        outcome,
        MergeOutcome::Merged,
        "the root's own links were all processed"
    );
    let heads = collection_heads(&db).await;
    assert!(
        heads.iter().any(|key| key.contains(&root_cid.to_string())),
        "the root's head must install: {heads:?}"
    );
}

/// Tolerating an incomplete ancestor must not discharge it. The ancestor
/// records no head and is not marked merged, so a later delivery of that same
/// ancestor block walks it again and merges the document it names.
///
/// The root is a separate question: its own links were all held, so it merges
/// and is discharged. Re-driving the root therefore does not reach the
/// ancestor — the walk prunes at a merged CID — which is why recovery depends
/// on the ancestor block arriving again rather than on a retry of the root.
#[tokio::test]
async fn an_incomplete_ancestor_merges_once_its_missing_link_arrives() {
    let (db, handler, blockstore) = node().await;

    let (absent, absent_blocks) = document("historical");
    let (ancestor_cid, ancestor_bytes) = collection_block(&[absent]);
    blockstore
        .put(&ancestor_cid, &ancestor_bytes)
        .await
        .unwrap();

    let (present, blocks) = document("current");
    for (cid, bytes) in &blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    let (root_cid, root_bytes) = collection_block_over(&[present], &[ancestor_cid]);
    blockstore.put(&root_cid, &root_bytes).await.unwrap();

    assert_eq!(
        merge(&handler, &root_cid, &root_bytes).await,
        MergeOutcome::Merged
    );
    assert!(
        !collection_heads(&db)
            .await
            .iter()
            .any(|key| key.contains(&ancestor_cid.to_string())),
        "an ancestor whose link went unprocessed must install no head"
    );

    // The missing document arrives, and the ancestor is delivered again.
    for (cid, bytes) in &absent_blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }

    merge(&handler, &ancestor_cid, &ancestor_bytes).await;

    let heads = collection_heads(&db).await;
    assert!(
        heads
            .iter()
            .any(|key| key.contains(&ancestor_cid.to_string())),
        "the ancestor was not discharged, so it merges when it arrives again: {heads:?}"
    );
}

async fn merge_batch(handler: &Handler, cid: &Cid, bytes: &[u8]) -> MergeOutcome {
    let results = handler
        .handle_block_batch(&[MergeBlock {
            cid: *cid,
            block_data: bytes.to_vec().into(),
            doc_id: String::new(),
            collection_id: COLLECTION_ID.to_string(),
            creator: "peer-did".to_string(),
            sender_peer: Some("peer".to_string()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }])
        .await;
    match results.into_iter().next() {
        Some(Ok(outcome)) => outcome,
        other => panic!("{other:?}"),
    }
}

/// The batch walk carries its own already-merged set, committed into the
/// process-wide one when the batch commits, so it needs the same rule: an
/// ancestor whose link went unprocessed records nothing and stays mergeable.
#[tokio::test]
async fn the_batch_walk_leaves_an_incomplete_ancestor_mergeable() {
    let (db, handler, blockstore) = node().await;

    let (absent, absent_blocks) = document("historical");
    let (ancestor_cid, ancestor_bytes) = collection_block(&[absent]);
    blockstore
        .put(&ancestor_cid, &ancestor_bytes)
        .await
        .unwrap();

    let (present, blocks) = document("current");
    for (cid, bytes) in &blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }
    let (root_cid, root_bytes) = collection_block_over(&[present], &[ancestor_cid]);
    blockstore.put(&root_cid, &root_bytes).await.unwrap();

    merge_batch(&handler, &root_cid, &root_bytes).await;

    assert!(
        !collection_heads(&db)
            .await
            .iter()
            .any(|key| key.contains(&ancestor_cid.to_string())),
        "the batch walk must install no head for an incomplete ancestor"
    );

    for (cid, bytes) in &absent_blocks {
        blockstore.put(cid, bytes).await.unwrap();
    }

    merge_batch(&handler, &ancestor_cid, &ancestor_bytes).await;

    let heads = collection_heads(&db).await;
    assert!(
        heads
            .iter()
            .any(|key| key.contains(&ancestor_cid.to_string())),
        "the batch walk left the ancestor mergeable: {heads:?}"
    );
}
