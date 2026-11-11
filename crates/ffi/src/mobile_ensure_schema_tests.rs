use std::ffi::{CStr, CString};
use std::sync::Arc;

use blockstore::Blockstore as _;
use cid::Cid;
use defra_core::merge::{BlockMetadata, MergeHandler as _};
use defra_core::Block;

use super::{
    defra_mobile_close_node, defra_mobile_ensure_schema, defra_mobile_init, defra_mobile_open_node,
};
use crate::state::{FfiBlockstore, FfiDatabase, FfiMergeHandler, NODES};
use crate::types::defra_free_string;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64, overlap: 1)
    }
"#;

/// Copies a definition block and its field blocks from `author` to `peer`,
/// then merges it there as a peer's block.
async fn sync_definition(author: &FfiBlockstore, peer: &Arc<FfiDatabase>, version_id: &str) {
    let peer_blockstore = Arc::new(FfiBlockstore::new(peer.store().clone(), false));
    let cid = Cid::try_from(version_id).unwrap();
    let bytes = author.get(&cid).await.unwrap().expect("definition block");
    let block = Block::from_dag_cbor(&bytes).unwrap();
    for link in block.links.iter().flatten() {
        let field = author.get(&link.link).await.unwrap().expect("field block");
        peer_blockstore.put(&link.link, &field).await.unwrap();
    }
    peer_blockstore.put(&cid, &bytes).await.unwrap();
    FfiMergeHandler::new(peer.clone(), peer_blockstore)
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();
}

fn take_string(ptr: *mut std::os::raw::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { defra_free_string(ptr) };
    text
}

#[test]
fn ensure_schema_skips_a_synced_inactive_chunked_collection() {
    let init = defra_mobile_init();
    take_string(init.value);
    let config = CString::new(r#"{"inMemory":true}"#).unwrap();
    let node = defra_mobile_open_node(config.as_ptr());
    assert_eq!(node.status, 0, "mobile open should succeed");
    let database = NODES
        .get(node.node_ptr, |state| state.database.clone())
        .unwrap();

    let parsed = query::parse_sdl(SDL).unwrap();
    workers::block_on(async {
        let author_store = Arc::new(storage::RegolithStore::in_memory().unwrap());
        let author = FfiDatabase::from_arc(author_store.clone()).unwrap();
        author
            .create_collections_atomic(parsed.clone())
            .await
            .unwrap();
        let author_blockstore = FfiBlockstore::new(author_store, false);
        for collection in &parsed {
            sync_definition(&author_blockstore, &database, &collection.version_id).await;
        }
    })
    .unwrap();
    let stored = workers::block_on(database.get_all_collection_versions())
        .unwrap()
        .unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|version| !version.is_active));

    let schema = CString::new(SDL).unwrap();
    let ensure = defra_mobile_ensure_schema(node.node_ptr, schema.as_ptr());
    let error = take_string(ensure.error);
    let value = take_string(ensure.value);
    assert_eq!(ensure.status, 0, "ensure schema failed: {error}");
    let result: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert_eq!(result["created"], serde_json::json!([]));
    assert_eq!(
        result["skipped"],
        serde_json::json!(["Article", "Article__body_chunk"])
    );

    let close = defra_mobile_close_node(node.node_ptr);
    assert_eq!(close.status, 0, "mobile close should succeed");
}
