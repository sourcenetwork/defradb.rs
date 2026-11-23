use std::sync::Arc;

use db::chunk::list::text_hash;
use db::database::DB;
use db::{AutoCommitMutator, DbBlockDocIDResolver};
use defra_core::block::generate_cid_from_bytes;
use document::{DocID, Document};
use kms::BlockDocIDResolver;
use query::mutator::DocMutator;
use storage::RegolithStore;

use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        body: String @chunk(maxChars: 64)
    }
    type Note {
        title: String
    }
"#;

async fn db_with(sdl: &str) -> Arc<DB<RegolithStore>> {
    let (db, _bus) = make_test_db_with_bus().await;
    db.add_collection(&defra_core::RequestContext::ambient(), sdl)
        .await
        .expect("schema");
    db
}

async fn chunked_db() -> Arc<DB<RegolithStore>> {
    db_with(SDL).await
}

async fn create(db: &Arc<DB<RegolithStore>>, collection: &str, json: &str) -> String {
    let version = db
        .get_collection(collection)
        .unwrap()
        .unwrap()
        .schema()
        .clone();
    AutoCommitMutator::new(Arc::clone(db))
        .create(collection, Document::from_json_str(json, version).unwrap())
        .await
        .unwrap()
        .doc_id
        .to_string()
}

/// A document under a collection root this node doesn't know, and its block.
async fn plant_unknown(db: &DB<RegolithStore>) -> (String, cid::Cid) {
    let doc_id = DocID::new_v0_from_seed("doc of an unknown collection").to_string();
    let block = generate_cid_from_bytes(doc_id.as_bytes()).unwrap();
    let unknown_root = u32::MAX;
    assert!(db.find_collection_by_root_id(unknown_root).is_none());

    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = db::docid::map::next_doc_short_id(&systemstore)
        .await
        .unwrap();
    db::docid::map::set_doc_id_mapping(&systemstore, unknown_root, short_id, &doc_id)
        .await
        .unwrap();
    db::docid::map::set_block_doc_id_mapping(&systemstore, &block.to_string(), &doc_id)
        .await
        .unwrap();
    drop(systemstore);
    txn.commit().await.unwrap();
    (doc_id, block)
}

async fn owners_with_collections(
    db: &Arc<DB<RegolithStore>>,
    doc_ids: Vec<String>,
) -> Vec<kms::BlockOwner> {
    let txn = db.new_txn(true).await.unwrap();
    let owners = db::chunk::access::read_owners(db, &txn, doc_ids)
        .await
        .unwrap();
    let _ = txn.discard();
    owners
}

fn chunks_collection_id(db: &DB<RegolithStore>) -> String {
    collection_id(db, "Article__body_chunk")
}

fn collection_id(db: &DB<RegolithStore>, name: &str) -> String {
    db.get_collection(name)
        .unwrap()
        .unwrap()
        .collection_id()
        .to_string()
}

/// Stores a chunk of `parent` the way a merge would, and returns its block's CID.
async fn plant_chunk(db: &DB<RegolithStore>, parent: &str) -> cid::Cid {
    let txn = db.new_txn(false).await.unwrap();
    let block = plant_chunk_in(db, &txn, parent).await;
    txn.commit().await.unwrap();
    block
}

async fn plant_chunk_in(
    db: &DB<RegolithStore>,
    txn: &db::DbTxn<RegolithStore>,
    parent: &str,
) -> cid::Cid {
    let chunks_col = db.get_collection("Article__body_chunk").unwrap().unwrap();
    let chunk_id = DocID::new_v0_from_seed(&format!("chunk of {parent}"));
    let mut chunk = Document::from_json_str(
        &format!(
            r#"{{"_sourceID": "{parent}", "text": "t", "hash": "{}"}}"#,
            text_hash("t")
        ),
        chunks_col.schema().clone(),
    )
    .unwrap();
    chunk.set_id(chunk_id.clone());
    let block = generate_cid_from_bytes(chunk_id.to_string().as_bytes()).unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = db::docid::map::next_doc_short_id(&systemstore)
        .await
        .unwrap();
    chunks_col
        .create_with_datastore(&txn.datastore().unwrap(), &chunk, short_id)
        .await
        .unwrap();
    let id = chunk_id.to_string();
    db::docid::map::set_doc_id_mapping(&systemstore, chunks_col.resolved_root_id(), short_id, &id)
        .await
        .unwrap();
    db::docid::map::set_block_doc_id_mapping(&systemstore, &block.to_string(), &id)
        .await
        .unwrap();
    drop(systemstore);
    block
}

#[test]
fn a_chunk_whose_parent_has_not_arrived_has_no_key_owner() {
    workers::block_on(async {
        let db = chunked_db().await;
        let missing = DocID::new_v0_from_seed("parent not merged yet").to_string();
        let block = plant_chunk(&db, &missing).await;

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .owners_of_block(&block)
            .await
            .unwrap();

        assert!(owners.is_empty(), "{owners:?}");
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_still_owns_its_old_chunks_keys() {
    workers::block_on(async {
        let db = chunked_db().await;
        let parent = create(&db, "Article", r#"{"body": "x"}"#).await;
        let block = plant_chunk(&db, &parent).await;
        AutoCommitMutator::new(Arc::clone(&db))
            .delete("Article", &DocID::from_string(&parent).unwrap())
            .await
            .unwrap();

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .owners_of_block(&block)
            .await
            .unwrap();

        assert_eq!(
            owners,
            vec![kms::BlockOwner {
                doc_id: parent,
                block_collection_id: chunks_collection_id(&db),
            }]
        );
    })
    .unwrap()
}

#[test]
fn chunk_parents_are_read_in_the_callers_transaction() {
    workers::block_on(async {
        let db = chunked_db().await;
        let chunks_col = db.get_collection("Article__body_chunk").unwrap().unwrap();
        let articles = db.get_collection("Article").unwrap().unwrap();
        let parent = DocID::new_v0_from_seed("uncommitted parent").to_string();
        let chunk_id = DocID::new_v0_from_seed(&format!("chunk of {parent}")).to_string();

        let txn = db.new_txn(false).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let short_id = db::docid::map::next_doc_short_id(&systemstore)
            .await
            .unwrap();
        db::docid::map::set_doc_id_mapping(
            &systemstore,
            articles.resolved_root_id(),
            short_id,
            &parent,
        )
        .await
        .unwrap();
        drop(systemstore);
        plant_chunk_in(&db, &txn, &parent).await;

        let parents = db::chunk::access::history_parents(
            &db,
            &txn,
            chunks_col.schema(),
            std::slice::from_ref(&chunk_id),
        )
        .await
        .unwrap();
        let _ = txn.discard();

        assert_eq!(parents, vec![parent]);
    })
    .unwrap()
}

#[test]
fn a_document_whose_collection_is_unknown_has_no_key_owner_on_a_node_without_chunks() {
    workers::block_on(async {
        let db = db_with("type Note { title: String }").await;
        let (_, block) = plant_unknown(&db).await;

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .owners_of_block(&block)
            .await
            .unwrap();

        assert!(owners.is_empty(), "{owners:?}");
    })
    .unwrap()
}

#[test]
fn a_mixed_list_keeps_plain_documents_and_maps_chunks_to_their_parents() {
    workers::block_on(async {
        let db = chunked_db().await;
        let note = create(&db, "Note", r#"{"title": "n"}"#).await;
        let parent = create(&db, "Article", r#"{"body": "x"}"#).await;
        plant_chunk(&db, &parent).await;
        let chunk_id = DocID::new_v0_from_seed(&format!("chunk of {parent}")).to_string();
        let (unknown, _) = plant_unknown(&db).await;

        let owners = owners_with_collections(&db, vec![note.clone(), chunk_id, unknown]).await;

        let expected = vec![
            kms::BlockOwner {
                doc_id: note,
                block_collection_id: collection_id(&db, "Note"),
            },
            kms::BlockOwner {
                doc_id: parent,
                block_collection_id: chunks_collection_id(&db),
            },
        ];
        assert_eq!(owners, expected);
    })
    .unwrap()
}

/// Points `doc_id` at a collection root the cache doesn't hold, keeping its stored blocks.
async fn uncache_collection(db: &DB<RegolithStore>, doc_id: &str) {
    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let doc_ref = db::docid::map::get_doc_ref(&systemstore, doc_id)
        .await
        .unwrap()
        .unwrap();
    let unknown_root = u32::MAX;
    assert!(db.find_collection_by_root_id(unknown_root).is_none());
    db::docid::map::set_doc_id_mapping(&systemstore, unknown_root, doc_ref.doc_short_id, doc_id)
        .await
        .unwrap();
    drop(systemstore);
    txn.commit().await.unwrap();
}

#[test]
fn a_stored_plain_document_the_cache_misses_owns_itself() {
    workers::block_on(async {
        let db = chunked_db().await;
        let note = create(&db, "Note", r#"{"title": "n"}"#).await;
        uncache_collection(&db, &note).await;

        let owners = owners_with_collections(&db, vec![note.clone()]).await;
        let expected = vec![kms::BlockOwner {
            doc_id: note,
            block_collection_id: collection_id(&db, "Note"),
        }];
        assert_eq!(owners, expected);
    })
    .unwrap()
}

#[test]
fn a_stored_chunk_the_cache_misses_has_no_owner() {
    workers::block_on(async {
        let db = chunked_db().await;
        let parent = create(&db, "Article", r#"{"body": "x"}"#).await;
        let chunks_col = db.get_collection("Article__body_chunk").unwrap().unwrap();
        let chunk = Document::from_json_str(
            &format!(
                r#"{{"_sourceID": "{parent}", "text": "t", "hash": "{}"}}"#,
                text_hash("t")
            ),
            chunks_col.schema().clone(),
        )
        .unwrap();
        let txn = db.new_txn(false).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let short_id = db::docid::map::next_doc_short_id(&systemstore)
            .await
            .unwrap();
        let created = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &chunk,
            chunks_col.schema(),
            db::block::builder::DocRef::new(chunks_col.resolved_root_id(), short_id),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        db::docid::map::set_doc_id_mapping(
            &systemstore,
            chunks_col.resolved_root_id(),
            short_id,
            &created.doc_id,
        )
        .await
        .unwrap();
        drop(systemstore);
        txn.commit().await.unwrap();
        uncache_collection(&db, &created.doc_id).await;

        assert!(owners_with_collections(&db, vec![created.doc_id])
            .await
            .is_empty());
    })
    .unwrap()
}
