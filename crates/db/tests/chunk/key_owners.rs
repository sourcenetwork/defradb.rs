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
"#;

async fn chunked_db() -> Arc<DB<RegolithStore>> {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .expect("schema");
    db
}

/// Stores a chunk of `parent` the way a merge would, and returns its block's CID.
async fn plant_chunk(db: &DB<RegolithStore>, parent: &str) -> cid::Cid {
    let chunks_col = db.get_collection("Article__body_chunk").unwrap().unwrap();
    let chunk_id = DocID::new_v0_from_seed(&format!("chunk of {parent}"));
    let mut chunk = Document::from_json_str(
        &format!(r#"{{"_sourceID": "{parent}", "text": "t", "hash": "00"}}"#),
        chunks_col.schema().clone(),
    )
    .unwrap();
    chunk.set_id(chunk_id.clone());
    let block = generate_cid_from_bytes(chunk_id.to_string().as_bytes()).unwrap();

    let txn = db.new_txn(false).await.unwrap();
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
    txn.commit().await.unwrap();
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
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let version = db
            .get_collection("Article")
            .unwrap()
            .unwrap()
            .schema()
            .clone();
        let parent = mutator
            .create(
                "Article",
                Document::from_json_str(r#"{"body": "x"}"#, version).unwrap(),
            )
            .await
            .unwrap()
            .doc_id
            .to_string();
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

        let parents = db::chunk::access::history_parents(&db, chunks_col.schema(), &[chunk_id])
            .await
            .unwrap();

        assert!(parents.is_empty(), "{parents:?}");
    })
    .unwrap()
}
