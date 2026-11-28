use db::database::DB;
use db::merge::replication::*;
use db::AutoCommitMutator;
use document::Document;
use query::mutator::DocMutator;
use std::sync::Arc;
use storage::RegolithStore;

#[test]
fn load_document_head_blocks_returns_current_composite_block() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        db.add_collection(
            &defra_core::RequestContext::ambient(),
            "type Transcript { body: String idx: Int }",
        )
        .await
        .unwrap();

        let mutator = AutoCommitMutator::new(db.clone());
        let result = mutator
            .create_many("Transcript", vec![make_transcript(&db, "first", 1)])
            .await
            .unwrap()
            .pop()
            .unwrap();

        let doc_id = result.doc_id.to_string();
        let commit_cid = result.commit_cid.expect("commit cid");
        let commit_block = result.commit_block.expect("commit block");

        let blocks = load_document_head_blocks(&db, &doc_id).await.unwrap();

        assert_eq!(blocks, vec![(commit_cid, commit_block)]);
    })
    .unwrap()
}

fn make_transcript(db: &Arc<DB<RegolithStore>>, body: &str, idx: i64) -> Document {
    let transcript = db
        .get_collection("Transcript")
        .unwrap()
        .unwrap()
        .schema_arc();
    let mut doc = Document::new(transcript);
    doc.set("body", body.to_string()).unwrap();
    doc.set("idx", idx).unwrap();
    doc
}
