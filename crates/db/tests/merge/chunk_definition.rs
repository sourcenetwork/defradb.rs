//! A peer that learns a `@chunk` schema from its definition blocks gets the
//! same chunk settings the author parsed.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeHandler};
use storage::RegolithStore;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64, overlap: 1)
    }
"#;

struct Node {
    db: Arc<DB<RegolithStore>>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
}

fn node() -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    Node { db, blockstore }
}

/// Copies a definition block and the field blocks it links from `author` to
/// `peer`, then merges it there.
async fn sync_definition(author: &Node, peer: &Node, version_id: &str) {
    let cid = Cid::try_from(version_id).unwrap();
    let bytes = author
        .blockstore
        .get(&cid)
        .await
        .unwrap()
        .expect("definition block");
    let block = Block::from_dag_cbor(&bytes).unwrap();
    for link in block.links.iter().flatten() {
        let field = author
            .blockstore
            .get(&link.link)
            .await
            .unwrap()
            .expect("field block");
        peer.blockstore.put(&link.link, &field).await.unwrap();
    }
    peer.blockstore.put(&cid, &bytes).await.unwrap();
    DbMergeHandler::new(peer.db.clone(), peer.blockstore.clone())
        .handle_block(
            &cid,
            &bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap();
}

#[test]
fn a_peer_rebuilds_chunk_settings_from_definition_blocks() {
    workers::block_on(async {
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
        for collection in &parsed {
            let synced = peer
                .db
                .get_collection_by_version_id_full(&collection.version_id)
                .await
                .unwrap()
                .expect("synced definition");
            assert_eq!(synced.schema().chunked_fields, collection.chunked_fields);
            assert_eq!(synced.schema().chunk_of, collection.chunk_of);
        }
        assert_eq!(parsed[0].chunked_fields[0].max_chars, 64);
        assert!(parsed[1].chunk_of.is_some());
    })
    .unwrap()
}
