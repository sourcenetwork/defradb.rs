use std::sync::Arc;

use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use db::AutoCommitMutator;
use document::Document;
use query::mutator::{DocMutator, MutationBatchOperation};
use query::txn::TransactionRegistry;
use rapidhash::{HashSetExt, RapidHashSet};
use storage::RegolithStore;

use crate::common::fixture::make_test_db_with_bus;
use crate::common::owned::in_txn;

const SDL: &str = r#"
    type Article {
        title: String
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

fn doc(db: &DB<RegolithStore>, collection: &str, json: &str) -> Document {
    let version = db
        .get_collection(collection)
        .unwrap()
        .unwrap()
        .schema()
        .clone();
    Document::from_json_str(json, version).expect("doc")
}

/// Tries every write a user must not make and returns the errors.
async fn forbidden_writes(db: &DB<RegolithStore>, mutator: &dyn DocMutator) -> Vec<String> {
    let mut errors = Vec::new();
    let forged = doc(
        db,
        "Article__body_chunk",
        r#"{"text": "forged", "hash": "00"}"#,
    );
    errors.push(
        mutator
            .create("Article__body_chunk", forged)
            .await
            .map(|_| ()),
    );
    let listed = doc(
        db,
        "Article",
        r#"{"body": "x", "_body_chunks": ["aa", "bb"]}"#,
    );
    errors.push(mutator.create("Article", listed).await.map(|_| ()));

    let created = mutator
        .create("Article", doc(db, "Article", r#"{"body": "x"}"#))
        .await
        .expect("a plain create");
    let mut update = doc(db, "Article", r#"{"_body_chunks": ["aa", "bb"]}"#);
    update.set_id(created.doc_id);
    let mut modified = RapidHashSet::new();
    modified.insert("_body_chunks".to_string());
    errors.push(
        mutator
            .update("Article", update, modified)
            .await
            .map(|_| ()),
    );
    errors
        .into_iter()
        .map(|result| result.expect_err("refused").to_string())
        .collect()
}

fn assert_refused(errors: &[String]) {
    assert!(
        errors[0].contains("written only by writes to Article"),
        "{}",
        errors[0]
    );
    assert!(errors[1].contains("read-only"), "{}", errors[1]);
    assert!(errors[2].contains("read-only"), "{}", errors[2]);
}

#[test]
fn autocommit_writes_cannot_set_chunks_or_lists() {
    workers::block_on(async {
        let db = chunked_db().await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        assert_refused(&forbidden_writes(&db, &mutator).await);
    })
    .unwrap()
}

#[test]
fn explicit_transaction_writes_cannot_set_chunks_or_lists() {
    workers::block_on(async {
        let db = chunked_db().await;
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin(false).await.expect("begin");
        let errors = in_txn(&registry, &handle, {
            let db = Arc::clone(&db);
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");
                    forbidden_writes(&db, mutator.as_ref()).await
                })
            }
        })
        .await;
        assert_refused(&errors);
    })
    .unwrap()
}

#[test]
fn batch_writes_cannot_set_chunks_or_lists() {
    workers::block_on(async {
        let db = chunked_db().await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let batch = mutator.begin_batch().await.unwrap().unwrap();
        let operation: MutationBatchOperation = Arc::new({
            let db = Arc::clone(&db);
            move |mutator, _| {
                let db = Arc::clone(&db);
                Box::pin(async move {
                    let errors = forbidden_writes(&db, mutator.as_ref()).await;
                    Ok(serde_json::json!(errors))
                })
            }
        });
        let errors: Vec<String> =
            serde_json::from_value(batch.run(operation).await.unwrap()).unwrap();
        assert_refused(&errors);
    })
    .unwrap()
}
