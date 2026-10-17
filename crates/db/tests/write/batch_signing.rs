use crate::common::{fixture::make_test_db_with_bus, schema::test_collection};
use db::AutoCommitMutator;
use defra_core::{batch_signing, Block};
use document::Document;
use query::mutator::DocMutator;

#[test]
fn create_many_collects_field_and_composite_cids_for_both_documents() {
    workers::block_on(async {
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
    })
    .unwrap()
}

async fn collect_failed_write<F, Fut>(session: &str, write: F) -> Vec<cid::Cid>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    batch_signing::batch_start(session);
    batch_signing::set_batch_session_key(Some(session.to_string()));
    let failed = write().await;
    batch_signing::set_batch_session_key(None);
    assert!(failed, "the write must fail and roll back");
    batch_signing::batch_take_cids(session).unwrap()
}

#[test]
fn create_many_rollback_collects_no_cids() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let collected = collect_failed_write("create-many-rollback", || async {
            mutator
                .create_many(
                    "TestDoc",
                    vec![
                        Document::from_json_str(r#"{"x":1}"#).unwrap(),
                        Document::from_json_str(r#"{"x":1}"#).unwrap(),
                    ],
                )
                .await
                .is_err()
        })
        .await;
        assert_eq!(collected, Vec::new());
    })
    .unwrap()
}

#[test]
fn create_rollback_collects_no_cids() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let doc = || Document::from_json_str(r#"{"x":1}"#).unwrap();
        mutator.create("TestDoc", doc()).await.unwrap();
        let collected = collect_failed_write("create-rollback", || async {
            mutator.create("TestDoc", doc()).await.is_err()
        })
        .await;
        assert_eq!(collected, Vec::new());
    })
    .unwrap()
}
