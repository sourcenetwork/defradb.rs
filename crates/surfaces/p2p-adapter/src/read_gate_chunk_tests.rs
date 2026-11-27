use std::sync::Arc;

use document::{Document, NormalValue};
use p2p::bitswap::{BlockClass, BlockClassifier};
use storage::RegolithStore;

use super::DbBlockClassifier;

const CHUNKS: &str = "Article__body_chunk";

async fn chunked_db() -> Arc<db::DB<RegolithStore>> {
    let db = Arc::new(db::DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    // The parent is under the policy; its chunk collection takes none.
    db.add_collection(
        &defra_core::RequestContext::ambient(),
        r#"type Article @policy(id: "chunk-policy", resource: "articles") {
            body: String @chunk(maxChars: 64)
        }"#,
    )
    .await
    .unwrap();
    db
}

/// Stores a chunk of `parent` as a merge would, and returns its composite block.
async fn plant_chunk(db: &db::DB<RegolithStore>, parent: &str) -> (cid::Cid, Vec<u8>) {
    let chunks = db.get_collection(CHUNKS).unwrap().unwrap();
    let mut chunk = Document::new(chunks.schema_arc());
    chunk
        .set("_sourceID", NormalValue::String(parent.into()))
        .unwrap();
    chunk.set("text", NormalValue::String("t".into())).unwrap();
    chunk
        .set("hash", NormalValue::String(db::chunk::hash::text_hash("t")))
        .unwrap();
    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = db.next_doc_short_id().await.unwrap();
    let created = db::block::builder::write_document_blocks(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        &chunk,
        chunks.schema(),
        db::block::builder::DocRef::new(chunks.resolved_root_id(), short_id),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_doc_id_mapping(
        &systemstore,
        chunks.resolved_root_id(),
        short_id,
        &created.doc_id,
    )
    .await
    .unwrap();
    db::docid::map::set_block_doc_id_mapping(
        &systemstore,
        &created.cid.to_string(),
        &created.doc_id,
    )
    .await
    .unwrap();
    chunk.set_id(document::DocID::from_string(&created.doc_id).unwrap());
    chunks
        .save_with_datastore(&txn.datastore().unwrap(), &chunk, short_id)
        .await
        .unwrap();
    let bytes = txn
        .blockstore()
        .unwrap()
        .get(&created.cid.to_bytes())
        .await
        .unwrap()
        .unwrap();
    drop(systemstore);
    txn.commit().await.unwrap();
    (created.cid, bytes.to_vec())
}

async fn plant_parent(db: &db::DB<RegolithStore>, parent: &str) {
    let articles = db.get_collection("Article").unwrap().unwrap();
    let txn = db.new_txn(false).await.unwrap();
    let short_id = db.next_doc_short_id().await.unwrap();
    db::docid::map::set_doc_id_mapping(
        &txn.systemstore().unwrap(),
        articles.resolved_root_id(),
        short_id,
        parent,
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
}

#[nuclei::test]
async fn a_chunk_block_is_classified_under_its_parents_collection_and_policy() {
    let db = chunked_db().await;
    let parent = document::DocID::new_v0_from_seed("parent").to_string();
    plant_parent(&db, &parent).await;
    let (cid, bytes) = plant_chunk(&db, &parent).await;
    let articles = db.get_collection("Article").unwrap().unwrap();

    let class = DbBlockClassifier::new(db).classify(&cid, &bytes).await;

    let BlockClass::Data(meta) = class else {
        panic!("expected a data block, got {class:?}");
    };
    assert_eq!(meta.collection_id, articles.collection_id());
    assert_eq!(
        meta.policy,
        Some(("chunk-policy".to_string(), "articles".to_string()))
    );
    assert_eq!(meta.doc_ids, vec![parent]);
}

#[nuclei::test]
async fn a_chunk_block_whose_parent_has_not_arrived_is_denied() {
    let db = chunked_db().await;
    let parent = document::DocID::new_v0_from_seed("parent not merged yet").to_string();
    let (cid, bytes) = plant_chunk(&db, &parent).await;

    let classifier = DbBlockClassifier::new(db);

    assert_eq!(classifier.classify(&cid, &bytes).await, BlockClass::Deny);
    assert!(classifier.classify_indexed(&cid).await.is_none());
}
