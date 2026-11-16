use std::sync::Arc;

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
    db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
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

async fn read_owners(db: &Arc<DB<RegolithStore>>, doc_ids: Vec<String>) -> Vec<String> {
    let txn = db.new_txn(true).await.unwrap();
    let owners = db::chunk::access::read_owners(db, &txn, doc_ids)
        .await
        .unwrap();
    let _ = txn.discard();
    owners
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
        &format!(r#"{{"_sourceID": "{parent}", "text": "t", "hash": "00"}}"#),
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
            .doc_ids_for_block(&block)
            .await
            .unwrap();

        assert!(owners.is_empty(), "{owners:?}");
    })
    .unwrap()
}

#[test]
fn a_chunks_key_owner_is_its_parent() {
    workers::block_on(async {
        let db = chunked_db().await;
        let parent = create(&db, "Article", r#"{"body": "x"}"#).await;
        let block = plant_chunk(&db, &parent).await;

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .doc_ids_for_block(&block)
            .await
            .unwrap();

        assert_eq!(owners, vec![parent]);
    })
    .unwrap()
}

#[test]
fn a_chunk_whose_parent_has_not_arrived_has_no_block_owner() {
    workers::block_on(async {
        let db = chunked_db().await;
        let missing = DocID::new_v0_from_seed("parent not merged yet").to_string();
        plant_chunk(&db, &missing).await;
        let chunks_col = db.get_collection("Article__body_chunk").unwrap().unwrap();
        let chunk_id = DocID::new_v0_from_seed(&format!("chunk of {missing}")).to_string();

        let txn = db.new_txn(true).await.unwrap();
        let parents =
            db::chunk::access::history_parents(&db, &txn, chunks_col.schema(), &[chunk_id])
                .await
                .unwrap();
        let _ = txn.discard();

        assert!(parents.is_empty(), "{parents:?}");
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
fn a_document_whose_collection_is_unknown_has_no_key_owner() {
    workers::block_on(async {
        let db = chunked_db().await;
        let (_, block) = plant_unknown(&db).await;

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .doc_ids_for_block(&block)
            .await
            .unwrap();

        assert!(owners.is_empty(), "{owners:?}");
    })
    .unwrap()
}

#[test]
fn a_document_whose_collection_is_unknown_has_no_key_owner_on_a_node_without_chunks() {
    workers::block_on(async {
        let db = db_with("type Note { title: String }").await;
        let (_, block) = plant_unknown(&db).await;

        let owners = DbBlockDocIDResolver::new(Arc::clone(&db))
            .doc_ids_for_block(&block)
            .await
            .unwrap();

        assert!(owners.is_empty(), "{owners:?}");
    })
    .unwrap()
}

#[test]
fn a_plain_documents_key_owner_is_itself_beside_chunk_collections() {
    workers::block_on(async {
        let db = chunked_db().await;
        let note = create(&db, "Note", r#"{"title": "n"}"#).await;

        assert_eq!(read_owners(&db, vec![note.clone()]).await, vec![note]);
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

        let owners = read_owners(&db, vec![note.clone(), chunk_id, unknown]).await;

        assert_eq!(owners, vec![note, parent]);
    })
    .unwrap()
}
