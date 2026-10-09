use crate::common::{fixture::make_test_db_with_bus, schema::test_collection};
use db::AutoCommitMutator;
use defra_core::{batch_signing, Block};
use document::Document;
use query::mutator::DocMutator;

#[tokio::test]
async fn create_many_collects_field_and_composite_cids_for_both_documents() {
    let (db, _) = make_test_db_with_bus().await;
    db.create_collection(test_collection()).await.unwrap();
    let session = "create-many-field-and-composite-cids";
    batch_signing::batch_start(session);
    batch_signing::set_batch_session_key(Some(session.to_string()));
    let result = AutoCommitMutator::new(db)
        .create_many(
            "TestDoc",
            vec![
                Document::from_json_str(r#"{"x":1}"#).unwrap(),
                Document::from_json_str(r#"{"x":2}"#).unwrap(),
            ],
        )
        .await;
    batch_signing::set_batch_session_key(None);
    let collected = batch_signing::batch_take_cids(session).unwrap();
    let results = result.unwrap();
    let mut expected = Vec::new();
    for result in results {
        expected.push(result.commit_cid.unwrap());
        let block = Block::from_dag_cbor(result.commit_block.as_ref().unwrap()).unwrap();
        expected.extend(block.links.unwrap().into_iter().map(|link| link.link));
    }
    assert_eq!(expected.len(), 4);
    expected.sort();
    let mut collected = collected;
    collected.sort();
    assert_eq!(collected, expected);
}
