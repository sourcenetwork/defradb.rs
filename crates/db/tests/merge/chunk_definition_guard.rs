//! A peer's definition block can't bring in `@chunk` settings the author's
//! schema rules refuse, or change them between versions.

use std::sync::Arc;

use blockstore::Blockstore as _;
use chunk_split::split::SplitterVersion;
use cid::Cid;
use db::merge::merge_handler::DbMergeHandler;
use db::txn::registry::DbTransactionRegistry;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::{
    Block, ChunkOfDelta, ChunkedFieldDelta, CollectionDefinitionDeltaPayload, CrdtDelta,
    RequestContext, DEFAULT_SPLITTER_VERSION,
};
use query::txn::TransactionRegistry;
use schema::{ChunkSettingsError, CollectionSource, CollectionVersion};
use storage::corekv::Key as _;
use storage::keys::systemstore::{CollectionKey, CollectionVersionKey};

use super::chunk_definition::{node, sync_definition, Node, SDL};

const PARENT: usize = 0;
const CHUNKS: usize = 1;
const CHANGED_FIELDS: &str = "Changed: the @chunk fields";
const CHANGED_PARENT: &str = "Changed: the chunk parent";
const CHANGED_SPLITTER: &str = "Changed: the @chunk splitter version";
const CHANGED_NAME: &str = "Changed: the collection name";
const RENAMED: &str = "Story";

/// An author with the `@chunk` schema and a peer that has synced both collections.
async fn synced() -> (Node, Node, Vec<CollectionVersion>) {
    let author = node();
    let parsed = query::parse_sdl(SDL).unwrap();
    author
        .db
        .create_collections_atomic(parsed.clone())
        .await
        .unwrap();
    let peer = node();
    for collection in &parsed {
        sync_definition(&author, &peer, &collection.version_id).await;
    }
    (author, peer, parsed)
}

/// Stores `block` on `peer` and merges it there.
async fn merge(peer: &Node, block: &Block) -> (Cid, MergeOutcome) {
    let bytes = block.to_dag_cbor().unwrap();
    let cid = block.generate_cid().unwrap();
    peer.blockstore.put(&cid, &bytes).await.unwrap();
    let outcome = DbMergeHandler::new(peer.db.clone(), peer.blockstore.clone())
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();
    (cid, outcome)
}

async fn author_block(author: &Node, version_id: &str) -> Block {
    let bytes = author
        .blockstore
        .get(&Cid::try_from(version_id).unwrap())
        .await
        .unwrap()
        .expect("definition block");
    Block::from_dag_cbor(&bytes).unwrap()
}

/// A patch of `previous` that adds no field and states `payload`'s chunk settings.
fn patch(previous: &str, payload: CollectionDefinitionDeltaPayload) -> Block {
    Block::new(
        CrdtDelta::CollectionDefinition(payload),
        vec![Cid::try_from(previous).unwrap()],
        Vec::new(),
    )
}

fn chunked(max_chars: u32) -> Vec<ChunkedFieldDelta> {
    chunked_on(max_chars, DEFAULT_SPLITTER_VERSION)
}

/// A version newer than any this build can split with.
fn unknown_splitter() -> u32 {
    SplitterVersion::ALL
        .iter()
        .map(|known| known.number())
        .max()
        .unwrap()
        + 1
}

fn chunked_on(max_chars: u32, splitter_version: u32) -> Vec<ChunkedFieldDelta> {
    vec![ChunkedFieldDelta {
        field: "body".to_string(),
        max_chars,
        overlap: 1,
        splitter_version,
    }]
}

fn payload(block: &Block) -> CollectionDefinitionDeltaPayload {
    match &block.delta {
        CrdtDelta::CollectionDefinition(payload) => payload.clone(),
        _ => panic!("not a definition block"),
    }
}

/// Checks the block was rejected, nothing was stored under it, and the
/// collection still holds only the versions it had.
async fn assert_rejected(
    peer: &Node,
    held: &[CollectionVersion],
    (cid, outcome): (Cid, MergeOutcome),
    reason: &str,
) {
    match &outcome {
        MergeOutcome::Rejected { reason: got } => assert!(got.contains(reason), "{got}"),
        other => panic!("expected a rejection, got {other:?}"),
    }
    assert!(peer
        .db
        .get_collection_by_version_id_full(&cid.to_string())
        .await
        .unwrap()
        .is_none());
    for version in held {
        let stored = peer
            .db
            .get_collection_by_version_id_full(&version.version_id)
            .await
            .unwrap()
            .expect("held version");
        assert_eq!(stored.schema().chunked_fields, version.chunked_fields);
        assert_eq!(stored.schema().chunk_of, version.chunk_of);
    }
    let versions = peer.db.get_all_collection_versions().await.unwrap();
    assert_eq!(versions.len(), held.len());
}

#[test]
fn a_patch_that_changes_max_chars_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2).with_chunked_fields(chunked(128));
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_FIELDS).await;
    })
    .unwrap()
}

#[test]
fn a_patch_that_changes_the_splitter_version_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2)
            .with_chunked_fields(chunked_on(64, unknown_splitter()));
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_SPLITTER).await;
    })
    .unwrap()
}

/// A newer peer's rules are a valid definition. This node keeps it and refuses
/// only to cut or check its chunks.
#[test]
fn a_first_version_on_a_splitter_version_this_node_lacks_is_kept_but_not_cut() {
    workers::block_on(async {
        let author = node();
        let parsed = query::parse_sdl(SDL).unwrap();
        author
            .db
            .create_collections_atomic(parsed.clone())
            .await
            .unwrap();
        let peer = node();
        let mut block = author_block(&author, &parsed[PARENT].version_id).await;
        for link in block.links.iter().flatten() {
            let field = author.blockstore.get(&link.link).await.unwrap().unwrap();
            peer.blockstore.put(&link.link, &field).await.unwrap();
        }
        let delta = payload(&block).with_chunked_fields(chunked_on(64, unknown_splitter()));
        block.delta = CrdtDelta::CollectionDefinition(delta);
        let (cid, outcome) = merge(&peer, &block).await;
        assert!(
            !matches!(outcome, MergeOutcome::Rejected { .. }),
            "{outcome:?}"
        );
        assert_ne!(cid.to_string(), parsed[PARENT].version_id);
        let stored = peer
            .db
            .get_collection_by_version_id_full(&cid.to_string())
            .await
            .unwrap()
            .expect("stored first version");
        let body = stored.schema().chunked_field("body").unwrap().clone();
        assert_eq!(body.splitter_version, unknown_splitter());
        match db::chunk::chunk_config("Article", &body) {
            Err(db::error::Error::UnknownSplitterVersion { source, .. }) => {
                assert_eq!(source.version, unknown_splitter())
            }
            other => panic!("expected an unknown splitter version, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
fn a_patch_with_max_chars_below_the_minimum_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2).with_chunked_fields(chunked(1));
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_FIELDS).await;
    })
    .unwrap()
}

#[test]
fn a_patch_that_empties_the_chunked_fields_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2);
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_FIELDS).await;
    })
    .unwrap()
}

#[test]
fn a_patch_that_adds_chunk_of_to_the_parent_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2)
            .with_chunked_fields(chunked(64))
            .with_chunk_of(ChunkOfDelta {
                collection: "Article".to_string(),
                field: "body".to_string(),
            });
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_PARENT).await;
    })
    .unwrap()
}

#[test]
fn a_patch_that_removes_chunk_of_from_the_chunk_collection_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let delta = CollectionDefinitionDeltaPayload::new(2);
        let merged = merge(&peer, &patch(&parsed[CHUNKS].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_PARENT).await;
    })
    .unwrap()
}

#[test]
fn a_patch_that_renames_the_parent_is_rejected() {
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let mut delta = CollectionDefinitionDeltaPayload::new(2).with_chunked_fields(chunked(64));
        delta.name = Some(RENAMED.to_string());
        let merged = merge(&peer, &patch(&parsed[PARENT].version_id, delta)).await;
        assert_rejected(&peer, &parsed, merged, CHANGED_NAME).await;
    })
    .unwrap()
}

#[test]
fn a_first_version_with_max_chars_below_the_minimum_is_rejected() {
    workers::block_on(async {
        let author = node();
        let parsed = query::parse_sdl(SDL).unwrap();
        author
            .db
            .create_collections_atomic(parsed.clone())
            .await
            .unwrap();
        let peer = node();
        let mut block = author_block(&author, &parsed[PARENT].version_id).await;
        for link in block.links.iter().flatten() {
            let field = author.blockstore.get(&link.link).await.unwrap().unwrap();
            peer.blockstore.put(&link.link, &field).await.unwrap();
        }
        let delta = payload(&block).with_chunked_fields(chunked(1));
        block.delta = CrdtDelta::CollectionDefinition(delta);
        let merged = merge(&peer, &block).await;
        assert_rejected(&peer, &[], merged, "maxChars must be at least 64, got 1").await;
    })
    .unwrap()
}

#[test]
fn a_first_version_with_chunk_of_on_the_parent_is_rejected() {
    workers::block_on(async {
        let author = node();
        let parsed = query::parse_sdl(SDL).unwrap();
        author
            .db
            .create_collections_atomic(parsed.clone())
            .await
            .unwrap();
        let peer = node();
        let mut block = author_block(&author, &parsed[PARENT].version_id).await;
        for link in block.links.iter().flatten() {
            let field = author.blockstore.get(&link.link).await.unwrap().unwrap();
            peer.blockstore.put(&link.link, &field).await.unwrap();
        }
        let delta = payload(&block).with_chunk_of(ChunkOfDelta {
            collection: "Article".to_string(),
            field: "body".to_string(),
        });
        block.delta = CrdtDelta::CollectionDefinition(delta);
        let merged = merge(&peer, &block).await;
        assert_rejected(
            &peer,
            &[],
            merged,
            "whose chunk collection is Article__body_chunk",
        )
        .await;
    })
    .unwrap()
}

#[test]
fn a_chunk_collection_naming_a_parent_field_without_chunk_is_rejected() {
    workers::block_on(async {
        let (author, peer, parsed) = synced().await;
        let mut block = author_block(&author, &parsed[CHUNKS].version_id).await;
        let mut delta = payload(&block).with_chunk_of(ChunkOfDelta {
            collection: "Article".to_string(),
            field: "title".to_string(),
        });
        delta.name = Some("Article__title_chunk".to_string());
        block.delta = CrdtDelta::CollectionDefinition(delta);
        let merged = merge(&peer, &block).await;
        assert_rejected(
            &peer,
            &parsed,
            merged,
            "Article.title as its parent, which has no @chunk",
        )
        .await;
    })
    .unwrap()
}

#[test]
fn an_honest_patch_that_keeps_the_chunk_settings_is_accepted() {
    workers::block_on(async {
        let (author, peer, parsed) = synced().await;
        author
            .db
            .patch_collection(
                &RequestContext::default(),
                r#"[{"op": "add", "path": "/Article/Fields/-", "value": {"Name": "summary", "Kind": "String"}}]"#,
                None,
            )
            .await
            .unwrap();
        let patched = author.db.get_collection("Article").unwrap().unwrap();
        sync_definition(&author, &peer, patched.version_id()).await;
        let synced = peer
            .db
            .get_collection_by_version_id_full(patched.version_id())
            .await
            .unwrap()
            .expect("synced patch");
        assert_eq!(synced.schema().chunked_fields, parsed[PARENT].chunked_fields);
        assert!(synced.schema().field_by_name("summary").is_some());
    })
    .unwrap()
}

/// A version stored with other settings than its collection's first, as a
/// node that merged it before the merge checked them would hold it.
async fn store_drifted_version<F>(peer: &Node, first: &CollectionVersion, drift: F) -> String
where
    F: FnOnce(&mut CollectionVersion),
{
    let mut drifted = first.clone();
    drifted.version_id = "bafyreidriftedchunksettingsversion".to_string();
    drifted.previous_version = Some(CollectionSource::new(first.version_id.clone()));
    drift(&mut drifted);
    drifted.is_active = false;
    let txn = peer.db.new_txn(false).await.unwrap();
    {
        let systemstore = txn.systemstore().unwrap();
        systemstore
            .set(
                &CollectionKey::new(&drifted.version_id).bytes(),
                &serde_json::to_vec(&drifted).unwrap(),
            )
            .await
            .unwrap();
        systemstore
            .set(
                &CollectionVersionKey::new(&drifted.collection_id, &drifted.version_id).bytes(),
                b"1",
            )
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    drifted.version_id
}

fn assert_settings_refused<T>(result: Result<T, db::error::Error>)
where
    T: std::fmt::Debug,
{
    match result {
        Err(db::error::Error::ChunkSettings(ChunkSettingsError::Changed { .. })) => {}
        other => panic!("expected a chunk settings refusal, got {other:?}"),
    }
}

#[test]
fn activating_a_version_with_other_chunk_settings_is_refused() {
    activating_a_drifted_version_is_refused(|version| version.chunked_fields[0].max_chars = 1);
}

#[test]
fn activating_a_version_with_another_splitter_version_is_refused() {
    activating_a_drifted_version_is_refused(|version| {
        version.chunked_fields[0].splitter_version = unknown_splitter()
    });
}

#[test]
fn activating_a_renamed_version_is_refused() {
    activating_a_drifted_version_is_refused(|version| version.name = RENAMED.to_string());
}

fn activating_a_drifted_version_is_refused<F>(drift: F)
where
    F: FnOnce(&mut CollectionVersion),
{
    workers::block_on(async {
        let (_author, peer, parsed) = synced().await;
        let drifted = store_drifted_version(&peer, &parsed[PARENT], drift).await;

        assert_settings_refused(peer.db.set_active_collection_version(&drifted).await);
        let registry = DbTransactionRegistry::new(Arc::clone(&peer.db));
        let handle = registry.begin(false).await.unwrap();
        assert_settings_refused(
            registry
                .set_collection_active_in_txn(handle.as_str(), &drifted, true)
                .await,
        );
        registry.discard(&handle).await.unwrap();

        peer.db
            .set_active_collection_version(&parsed[PARENT].version_id)
            .await
            .expect("the first version activates");
        let active = peer.db.get_collection("Article").unwrap().unwrap();
        assert_eq!(active.schema().version_id, parsed[PARENT].version_id);
    })
    .unwrap()
}
