use crate::common::{fixture::make_test_db_with_bus, schema::test_collection};
use db::AutoCommitMutator;
use defra_core::batch_signing::BatchScope;
use defra_core::{batch_signing, Block};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;

/// Create two documents in one session of `scope`; the CIDs it collected,
/// and the composite and field CIDs each document wrote.
fn create_two(scope: BatchScope, session: &str) -> (Vec<cid::Cid>, Vec<cid::Cid>, Vec<cid::Cid>) {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        batch_signing::batch_start_with(session, scope);
        batch_signing::set_batch_session_key(Some(session.to_string()));
        let result = AutoCommitMutator::new(db)
            .create_many(
                "TestDoc",
                vec![
                    Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap(),
                    Document::from_json_str(r#"{"x":2}"#, test_collection()).unwrap(),
                ],
            )
            .await;
        batch_signing::set_batch_session_key(None);
        let mut collected = batch_signing::batch_take_cids(session).unwrap();
        collected.sort();
        let (mut composites, mut fields) = (Vec::new(), Vec::new());
        for result in result.unwrap() {
            composites.push(result.commit_cid.unwrap());
            let block = Block::from_dag_cbor(result.commit_block.as_ref().unwrap()).unwrap();
            fields.extend(block.links.unwrap().into_iter().map(|link| link.link));
        }
        (collected, composites, fields)
    })
    .unwrap()
}

/// Go Shinzo's scope, the default: one composite CID per document, so a
/// Rust root matches a Go root over the same writes.
#[test]
fn create_many_collects_one_composite_cid_per_document() {
    let (collected, mut composites, _) = create_two(BatchScope::Composite, "scope-composite");
    composites.sort();
    assert_eq!(collected, composites);
}

/// The opt-in full scope covers the field blocks too.
#[test]
fn create_many_in_full_scope_collects_field_and_composite_cids() {
    let (collected, composites, fields) = create_two(BatchScope::Full, "scope-full");
    let mut expected: Vec<_> = composites.into_iter().chain(fields).collect();
    assert_eq!(expected.len(), 4);
    expected.sort();
    assert_eq!(collected, expected);
}

/// An update's field blocks sit above priority 1 and are not signed one by
/// one, so the full scope must still cover them.
#[test]
fn update_in_full_scope_collects_its_field_and_composite_cids() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let created = mutator
            .create(
                "TestDoc",
                Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap(),
            )
            .await
            .unwrap();
        let mut update = Document::new(test_collection());
        update.set_id(created.doc_id.clone());
        update.set("x", NormalValue::Int(2)).unwrap();

        batch_signing::batch_start_with("scope-full-update", BatchScope::Full);
        batch_signing::set_batch_session_key(Some("scope-full-update".to_string()));
        let updated = mutator
            .update(
                "TestDoc",
                update,
                RapidHashSet::from_iter(["x".to_string()]),
            )
            .await;
        batch_signing::set_batch_session_key(None);
        let mut collected = batch_signing::batch_take_cids("scope-full-update").unwrap();
        collected.sort();

        let updated = updated.unwrap();
        let block = Block::from_dag_cbor(updated.commit_block.as_ref().unwrap()).unwrap();
        let mut expected: Vec<_> = block.links.unwrap().into_iter().map(|l| l.link).collect();
        assert_eq!(expected.len(), 1, "the update writes one field block");
        expected.push(updated.commit_cid.unwrap());
        expected.sort();
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
fn create_many_discard_collects_no_cids() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let collected = collect_failed_write("create-many-discard", || async {
            mutator
                .create_many(
                    "TestDoc",
                    vec![
                        Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap(),
                        Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap(),
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
fn create_discard_collects_no_cids() {
    workers::block_on(async {
        let (db, _) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db);
        let doc = || Document::from_json_str(r#"{"x":1}"#, test_collection()).unwrap();
        mutator.create("TestDoc", doc()).await.unwrap();
        let collected = collect_failed_write("create-discard", || async {
            mutator.create("TestDoc", doc()).await.is_err()
        })
        .await;
        assert_eq!(collected, Vec::new());
    })
    .unwrap()
}
