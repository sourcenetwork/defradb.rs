use crate::common::owned::in_txn;
use db::database::DB;
use db::txn::registry::*;
use document::Document;
use document::NormalValue;
use lens::LensConfig;
use lens::LensModule;
use query::mutator::DocMutator;
use query::txn::txn_job;
use query::txn::TransactionHandle;
use query::txn::TransactionRegistry;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use std::sync::Arc;
use storage::RegolithStore;

fn test_schema() -> Vec<CollectionVersion> {
    vec![CollectionVersion::new(
        "Users",
        "v1",
        "col-users",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
            FieldDescription::new("3", "age", FieldKind::int()),
        ],
    )]
}

/// Create a test DB with collections pre-registered.
async fn test_db_with_collections() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    for schema in test_schema() {
        db.create_collection(schema).await.unwrap();
    }
    db
}

/// Create a Users doc through the interactive mutator inside an open txn.
/// Returns the txn (for the caller to commit or discard) and the derived DocID.
async fn seed_user_in_txn(
    db: &Arc<DB<RegolithStore>>,
    txn: db::txn::DbTxn<RegolithStore>,
    name: &str,
    age: Option<i64>,
) -> (db::txn::DbTxn<RegolithStore>, document::DocID) {
    let mutator = db::write::doc::DbDocMutator::new(db.clone(), txn);
    let mut doc = Document::new();
    doc.set("name", NormalValue::String(name.to_string()));
    if let Some(age) = age {
        doc.set("age", NormalValue::Int(age));
    }
    let result = mutator.create("Users", doc).await.unwrap();
    let txn = mutator.take_txn().await.unwrap();
    (txn, result.doc_id)
}

/// Create and commit a Users doc, returning its derived DocID.
async fn seed_committed_user(
    db: &Arc<DB<RegolithStore>>,
    name: &str,
    age: Option<i64>,
) -> document::DocID {
    let txn = db.new_txn(false).await.unwrap();
    let (txn, doc_id) = seed_user_in_txn(db, txn, name, age).await;
    txn.commit().await.unwrap();
    doc_id
}

/// Whether a request for `txn_id` still reaches its transaction.
async fn found(
    registry: &DbTransactionRegistry<RegolithStore>,
    txn_id: &TransactionHandle,
) -> bool {
    registry
        .run(txn_id, None, txn_job(|_| Box::pin(async {})))
        .await
        .is_ok()
}

/// Every document of `collection`, read by a request on `txn_id`.
async fn get_all(
    registry: &DbTransactionRegistry<RegolithStore>,
    txn_id: &TransactionHandle,
    collection: &'static str,
) -> query::error::Result<Vec<Document>> {
    in_txn(registry, txn_id, move |ctx| {
        Box::pin(async move { ctx.doc_fetcher().get_all(collection).await })
    })
    .await
}

fn empty_wasm_module() -> LensModule {
    LensModule::from_bytes(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00])
}

#[test]
fn test_begin_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        assert!(!txn_id.as_str().is_empty());
    })
    .unwrap()
}

#[test]
fn test_begin_readonly_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        assert!(
            in_txn(&registry, &txn_id, |ctx| Box::pin(async move {
                ctx.is_readonly()
            }))
            .await
        );
    })
    .unwrap()
}

#[test]
fn test_begin_readwrite_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        assert!(
            !in_txn(&registry, &txn_id, |ctx| Box::pin(async move {
                ctx.is_readonly()
            }))
            .await
        );
    })
    .unwrap()
}

#[test]
fn add_schema_in_txn_rejects_an_unpaired_relation() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db);
        let txn_id = registry.begin(false).await.unwrap();

        let error = registry
            .add_schema_in_txn(
                txn_id.as_str(),
                r#"
                type User {
                    posts: [Post] @relation(name: "user_posts")
                }

                type Post {
                    title: String
                }
            "#,
            )
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("relation missing field. Object: Post, RelationName: user_posts"),
            "unexpected error: {error}"
        );
    })
    .unwrap()
}

#[test]
fn test_transaction_id_matches() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        let id = in_txn(&registry, &txn_id, |ctx| {
            Box::pin(async move { ctx.id().to_owned() })
        })
        .await;
        assert_eq!(id, txn_id.as_str());
    })
    .unwrap()
}

#[test]
fn test_begin_and_commit() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        let result = registry.commit(&txn_id).await;
        assert!(result.is_ok());
    })
    .unwrap()
}

#[test]
fn test_begin_and_rollback() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        let result = registry.rollback(&txn_id).await;
        assert!(result.is_ok());
    })
    .unwrap()
}

#[test]
fn test_commit_nonexistent_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let nonexistent: TransactionHandle = "nonexistent".parse().unwrap();
        let result = registry.commit(&nonexistent).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    })
    .unwrap()
}

#[test]
fn test_rollback_nonexistent_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let nonexistent: TransactionHandle = "nonexistent".parse().unwrap();
        let result = registry.rollback(&nonexistent).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    })
    .unwrap()
}

#[test]
fn test_get_nonexistent_returns_not_found() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let nonexistent: TransactionHandle = "nonexistent".parse().unwrap();
        assert!(!found(&registry, &nonexistent).await);
    })
    .unwrap()
}

#[test]
fn test_double_commit_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        registry.commit(&txn_id).await.unwrap();

        let result = registry.commit(&txn_id).await;
        assert!(result.is_err());
    })
    .unwrap()
}

#[test]
fn test_double_rollback_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        registry.rollback(&txn_id).await.unwrap();

        let result = registry.rollback(&txn_id).await;
        assert!(result.is_err());
    })
    .unwrap()
}

#[test]
fn test_commit_after_rollback_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        registry.rollback(&txn_id).await.unwrap();

        let result = registry.commit(&txn_id).await;
        assert!(result.is_err());
    })
    .unwrap()
}

#[test]
fn test_rollback_after_commit_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        registry.commit(&txn_id).await.unwrap();

        let result = registry.rollback(&txn_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    })
    .unwrap()
}

#[test]
fn test_get_returns_not_found_after_commit() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        assert!(found(&registry, &txn_id).await);

        registry.commit(&txn_id).await.unwrap();
        assert!(!found(&registry, &txn_id).await);
    })
    .unwrap()
}

#[test]
fn test_get_returns_not_found_after_rollback() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(false).await.unwrap();
        assert!(found(&registry, &txn_id).await);

        registry.rollback(&txn_id).await.unwrap();
        assert!(!found(&registry, &txn_id).await);
    })
    .unwrap()
}

#[test]
fn test_set_migration_in_txn_is_only_visible_inside_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        let txn_id = registry.begin(false).await.unwrap();
        let config = LensConfig::new("users-v1", "users-v2", empty_wasm_module());

        let transform_id = registry
            .set_migration_in_txn(&txn_id, config, None)
            .await
            .unwrap();

        let global_lenses = db.lens_store().list().await.unwrap();
        assert!(
            !global_lenses.contains_key(&transform_id.to_string()),
            "uncommitted migration leaked into global lens list"
        );

        let txn_lenses = registry.list_lenses_in_txn(&txn_id).await.unwrap();
        assert!(
            txn_lenses.contains_key(&transform_id.to_string()),
            "transaction-local migration should be visible inside the transaction"
        );

        registry.rollback(&txn_id).await.unwrap();

        let global_after_rollback = db.lens_store().list().await.unwrap();
        assert!(
            !global_after_rollback.contains_key(&transform_id.to_string()),
            "rolled back migration leaked into global lens list"
        );
    })
    .unwrap()
}

#[test]
fn test_set_migration_in_txn_promotes_transform_on_commit() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        let txn_id = registry.begin(false).await.unwrap();
        let config = LensConfig::new("users-v1", "users-v2", empty_wasm_module());

        let transform_id = registry
            .set_migration_in_txn(&txn_id, config, None)
            .await
            .unwrap();

        registry.commit(&txn_id).await.unwrap();

        let global_lenses = db.lens_store().list().await.unwrap();
        assert!(
            global_lenses.contains_key(&transform_id.to_string()),
            "committed migration should be promoted into the global lens store"
        );
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_get_all_empty_collection() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let docs = fetcher.get_all("Users").await.unwrap();
                assert!(docs.is_empty());
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_get_all_unknown_collection() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher.get_all("NonExistent").await;
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("NonExistent"));
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_get_by_ids_empty() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher.get_by_ids("Users", &[]).await.unwrap();
                assert!(result.docs().is_empty());
                assert!(result.missing_ids().is_empty());
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_get_by_ids_invalid_id_treated_as_not_found() {
    workers::block_on(async {
        // Go DefraDB treats invalid doc IDs as "not found" rather than errors.
        // This matches behavior where querying for a non-existent ID returns empty results.
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher
                    .get_by_ids("Users", &["not-a-valid-docid".to_string()])
                    .await
                    .unwrap();
                // Invalid doc ID is treated as not found, not an error
                assert!(result.docs().is_empty());
                assert_eq!(result.missing_ids(), &["not-a-valid-docid".to_string()]);
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_is_consumed_returns_false_before_take() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();

        assert!(
            found(&registry, &txn_id).await,
            "Transaction should not be consumed before it ends"
        );

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_is_consumed_returns_true_after_take() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();

        // End the transaction
        registry.rollback(&txn_id).await.unwrap();

        assert!(
            !found(&registry, &txn_id).await,
            "Transaction should be consumed after it ends"
        );
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_after_txn_consumed_returns_error() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        // A fetcher kept past its request.
        let fetcher = in_txn(&registry, &txn_id, |ctx| {
            Box::pin(async move { ctx.doc_fetcher() })
        })
        .await;

        // Ending the transaction takes it back from every handle it was lent to.
        registry.rollback(&txn_id).await.unwrap();

        // Now try to use the fetcher - should fail with "transaction already consumed"
        let result = fetcher.get_all("Users").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("transaction already consumed"));
    })
    .unwrap()
}

#[test]
fn test_transaction_sees_committed_data() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Write data in a separate transaction
        seed_committed_user(&db, "Alice", Some(30)).await;

        // Read via registry
        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let docs = fetcher.get_all("Users").await.unwrap();
                assert_eq!(docs.len(), 1);
                assert_eq!(docs[0].get("name").unwrap().as_str(), Some("Alice"));
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_get_by_ids_returns_matching_docs() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Create two documents
        let doc1_id = seed_committed_user(&db, "Alice", None).await.to_string();
        seed_committed_user(&db, "Bob", None).await;

        // Query for just one document
        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher.get_by_ids("Users", &[doc1_id]).await.unwrap();
                assert_eq!(result.docs().len(), 1);
                assert!(result.missing_ids().is_empty());
                assert_eq!(
                    result.docs()[0].get("name").unwrap().as_str(),
                    Some("Alice")
                );
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_multiple_concurrent_transactions() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn1 = registry.begin(true).await.unwrap();
        let txn2 = registry.begin(true).await.unwrap();
        let txn3 = registry.begin(false).await.unwrap();

        assert!(found(&registry, &txn1).await);
        assert!(found(&registry, &txn2).await);
        assert!(found(&registry, &txn3).await);

        assert_ne!(txn1, txn2);
        assert_ne!(txn2, txn3);

        registry.rollback(&txn1).await.unwrap();
        registry.rollback(&txn2).await.unwrap();
        registry.rollback(&txn3).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_transaction_ids_are_unique() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let mut ids = Vec::new();
        for _ in 0..10 {
            let txn_id = registry.begin(true).await.unwrap();
            assert!(!ids.contains(&txn_id), "Duplicate ID: {}", txn_id);
            ids.push(txn_id);
        }

        for id in ids {
            registry.rollback(&id).await.unwrap();
        }
    })
    .unwrap()
}

#[test]
fn test_rollback_discards_uncommitted_writes() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let collection = db.get_collection("Users").unwrap().unwrap();

        // Write data in a transaction but rollback instead of commit
        let write_txn = db.new_txn(false).await.unwrap();
        let (write_txn, _doc_id) = seed_user_in_txn(&db, write_txn, "RollbackMe", Some(99)).await;

        write_txn.force_discard().unwrap();

        // Verify data was NOT persisted
        let read_txn = db.new_txn(true).await.unwrap();
        let datastore = read_txn.datastore().unwrap();
        let systemstore = read_txn.systemstore().unwrap();
        let all_docs = collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await
            .unwrap();
        assert!(
            all_docs.is_empty(),
            "Rolled-back data should not be visible, found {} docs",
            all_docs.len()
        );
        drop(datastore);
        drop(systemstore);
        read_txn.force_discard().unwrap();
    })
    .unwrap()
}

#[test]
fn test_transaction_does_not_see_uncommitted_writes() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Start a reader transaction FIRST
        let reader_txn_id = registry.begin(true).await.unwrap();

        // Start a writer transaction and write WITHOUT committing
        let write_txn = db.new_txn(false).await.unwrap();
        let (write_txn, _doc_id) = seed_user_in_txn(&db, write_txn, "Uncommitted", None).await;

        // Reader should NOT see the uncommitted write
        let docs = get_all(&registry, &reader_txn_id, "Users").await.unwrap();
        assert!(
            docs.is_empty(),
            "Reader should not see uncommitted writes (dirty read protection)"
        );

        write_txn.force_discard().unwrap();
        registry.rollback(&reader_txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_concurrent_parallel_transaction_operations() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = Arc::new(DbTransactionRegistry::new(db));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let reg = registry.clone();
                workers::spawn(None, "test", move || async move {
                    let txn_id = reg.begin(true).await.unwrap();
                    workers::delay(std::time::Duration::from_millis(1))
                        .unwrap()
                        .await
                        .unwrap();

                    assert!(
                        found(&reg, &txn_id).await,
                        "Task {} should find its transaction",
                        i
                    );

                    workers::delay(std::time::Duration::from_millis(1))
                        .unwrap()
                        .await
                        .unwrap();
                    reg.rollback(&txn_id).await.unwrap();

                    assert!(
                        !found(&reg, &txn_id).await,
                        "Task {} transaction should be gone after rollback",
                        i
                    );
                })
                .unwrap()
            })
            .collect();

        for handle in handles {
            handle.await.expect("Task should complete without panic");
        }
    })
    .unwrap()
}

#[test]
fn test_doc_fetcher_get_by_ids_unknown_collection() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher
                    .get_by_ids("NonExistent", &["some-id".to_string()])
                    .await;
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("NonExistent"));
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_get_by_ids_with_nonexistent_valid_id() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Create one document
        let existing_id = seed_committed_user(&db, "Exists", None).await.to_string();

        // Create a valid-format ID that doesn't exist
        let nonexistent_id = document::DocID::new_v0_from_seed("ghost-user").to_string();

        // Query for both
        let txn_id = registry.begin(true).await.unwrap();
        in_txn(&registry, &txn_id, move |ctx| {
            Box::pin(async move {
                let fetcher = ctx.doc_fetcher();

                let result = fetcher
                    .get_by_ids("Users", &[existing_id, nonexistent_id.clone()])
                    .await
                    .unwrap();

                assert_eq!(
                    result.docs().len(),
                    1,
                    "Should only return existing document"
                );
                assert_eq!(
                    result.docs()[0].get("name").unwrap().as_str(),
                    Some("Exists")
                );

                // Verify missing IDs are reported
                assert_eq!(
                    result.missing_ids().len(),
                    1,
                    "Should report one missing ID"
                );
                assert_eq!(result.missing_ids()[0], nonexistent_id);
                assert!(!result.is_complete(), "Result should not be complete");
            })
        })
        .await;

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_concurrent_commit_same_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = Arc::new(DbTransactionRegistry::new(db));

        let txn_id = registry.begin(false).await.unwrap();

        // Spawn two tasks trying to commit the same transaction
        let reg1 = registry.clone();
        let txn1 = txn_id.clone();
        let handle1 = workers::spawn(
            None,
            "test",
            move || async move { reg1.commit(&txn1).await },
        )
        .unwrap();

        let reg2 = registry.clone();
        let txn2 = txn_id.clone();
        let handle2 = workers::spawn(
            None,
            "test",
            move || async move { reg2.commit(&txn2).await },
        )
        .unwrap();

        let (r1, r2) = futures::join!(handle1, handle2);
        let results = [r1.unwrap(), r2.unwrap()];

        // Exactly one should succeed, one should fail
        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(successes, 1, "Exactly one commit should succeed");
        assert_eq!(failures, 1, "Exactly one commit should fail");
    })
    .unwrap()
}

#[test]
fn test_concurrent_rollback_same_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = Arc::new(DbTransactionRegistry::new(db));

        let txn_id = registry.begin(false).await.unwrap();

        // Spawn two tasks trying to rollback the same transaction
        let reg1 = registry.clone();
        let txn1 = txn_id.clone();
        let handle1 = workers::spawn(
            None,
            "test",
            move || async move { reg1.rollback(&txn1).await },
        )
        .unwrap();

        let reg2 = registry.clone();
        let txn2 = txn_id.clone();
        let handle2 = workers::spawn(
            None,
            "test",
            move || async move { reg2.rollback(&txn2).await },
        )
        .unwrap();

        let (r1, r2) = futures::join!(handle1, handle2);
        let results = [r1.unwrap(), r2.unwrap()];

        // Exactly one should succeed, one should fail
        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(successes, 1, "Exactly one rollback should succeed");
        assert_eq!(failures, 1, "Exactly one rollback should fail");
    })
    .unwrap()
}

#[test]
fn test_concurrent_commit_and_rollback_same_transaction() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = Arc::new(DbTransactionRegistry::new(db));

        let txn_id = registry.begin(false).await.unwrap();

        // Spawn one task trying to commit, another trying to rollback
        let reg1 = registry.clone();
        let txn1 = txn_id.clone();
        let handle1 = workers::spawn(
            None,
            "test",
            move || async move { reg1.commit(&txn1).await },
        )
        .unwrap();

        let reg2 = registry.clone();
        let txn2 = txn_id.clone();
        let handle2 = workers::spawn(
            None,
            "test",
            move || async move { reg2.rollback(&txn2).await },
        )
        .unwrap();

        let (r1, r2) = futures::join!(handle1, handle2);
        let results = [r1.unwrap(), r2.unwrap()];

        // Exactly one should succeed, one should fail
        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(successes, 1, "Exactly one operation should succeed");
        assert_eq!(failures, 1, "Exactly one operation should fail");
    })
    .unwrap()
}

#[test]
fn test_cleanup_stale_transactions() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        // Begin some transactions
        let _txn1 = registry.begin(true).await.unwrap();
        let _txn2 = registry.begin(false).await.unwrap();

        assert_eq!(registry.active_transaction_count().unwrap(), 2);

        // Wait a bit so transactions become "stale"
        workers::delay(std::time::Duration::from_millis(50))
            .unwrap()
            .await
            .unwrap();

        // Cleanup with a very short max_age (0 means everything is stale)
        let result = registry
            .cleanup_stale_transactions(std::time::Duration::from_millis(0))
            .await
            .unwrap();

        assert_eq!(result.cleaned, 2, "Should have cleaned up 2 transactions");
        assert!(result.is_complete(), "All cleanups should succeed");
        assert_eq!(
            registry.active_transaction_count().unwrap(),
            0,
            "No transactions should remain"
        );
    })
    .unwrap()
}

#[test]
fn test_cleanup_only_old_transactions() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        // Begin an "old" transaction
        let _old_txn = registry.begin(true).await.unwrap();

        // Wait a bit
        workers::delay(std::time::Duration::from_millis(50))
            .unwrap()
            .await
            .unwrap();

        // Begin a "new" transaction
        let new_txn = registry.begin(true).await.unwrap();

        assert_eq!(registry.active_transaction_count().unwrap(), 2);

        // Cleanup with max_age that only catches the old transaction
        let result = registry
            .cleanup_stale_transactions(std::time::Duration::from_millis(40))
            .await
            .unwrap();

        assert_eq!(
            result.cleaned, 1,
            "Should have cleaned up 1 old transaction"
        );
        assert!(result.is_complete(), "All cleanups should succeed");
        assert_eq!(
            registry.active_transaction_count().unwrap(),
            1,
            "One new transaction should remain"
        );

        // The new transaction should still be usable
        assert!(found(&registry, &new_txn).await);
        registry.rollback(&new_txn).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_cleanup_uses_idle_age_not_creation_age() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        let txn = registry.begin(true).await.unwrap();
        workers::delay(std::time::Duration::from_millis(30))
            .unwrap()
            .await
            .unwrap();

        assert!(found(&registry, &txn).await);
        workers::delay(std::time::Duration::from_millis(20))
            .unwrap()
            .await
            .unwrap();

        let result = registry
            .cleanup_stale_transactions(std::time::Duration::from_millis(40))
            .await
            .unwrap();

        assert_eq!(
            result.cleaned, 0,
            "Recently used transaction should not be cleaned by creation age"
        );
        assert!(found(&registry, &txn).await);

        workers::delay(std::time::Duration::from_millis(45))
            .unwrap()
            .await
            .unwrap();

        let result = registry
            .cleanup_stale_transactions(std::time::Duration::from_millis(40))
            .await
            .unwrap();

        assert_eq!(
            result.cleaned, 1,
            "Transaction should be cleaned after it is idle past the limit"
        );
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

#[test]
fn test_periodic_cleanup_task_removes_idle_transactions() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = Arc::new(DbTransactionRegistry::new(db));

        let _txn = registry.begin(true).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 1);

        let cleanup_task = registry.start_stale_transaction_cleanup(
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(25),
        );

        workers::delay(std::time::Duration::from_millis(250))
            .unwrap()
            .await
            .unwrap();
        cleanup_task.abort();

        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

#[test]
fn test_active_transaction_count() {
    workers::block_on(async {
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db);

        assert_eq!(registry.active_transaction_count().unwrap(), 0);

        let txn1 = registry.begin(true).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 1);

        let txn2 = registry.begin(false).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 2);

        registry.commit(&txn1).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 1);

        registry.rollback(&txn2).await.unwrap();
        assert_eq!(registry.active_transaction_count().unwrap(), 0);
    })
    .unwrap()
}

#[test]
fn test_snapshot_isolation_after_external_commit() {
    workers::block_on(async {
        // This test verifies snapshot isolation: a transaction started before
        // another transaction commits should NOT see the committed data.
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Step 1: Start reader transaction A FIRST (gets snapshot at this point)
        let reader_txn_id = registry.begin(true).await.unwrap();

        // Verify initially empty
        let initial_docs = get_all(&registry, &reader_txn_id, "Users").await.unwrap();
        assert!(
            initial_docs.is_empty(),
            "Reader should see empty collection initially"
        );

        // Step 2: In a separate transaction, write and COMMIT data
        seed_committed_user(&db, "CommittedData", Some(42)).await;

        // Step 3: Reader transaction A should STILL see empty (snapshot isolation)
        // because its snapshot was taken before the write committed
        let after_commit_docs = get_all(&registry, &reader_txn_id, "Users").await.unwrap();
        assert!(
            after_commit_docs.is_empty(),
            "Reader should NOT see committed data due to snapshot isolation (found {} docs)",
            after_commit_docs.len()
        );

        registry.rollback(&reader_txn_id).await.unwrap();

        // Step 4: A NEW transaction started after commit SHOULD see the data
        let new_reader_txn_id = registry.begin(true).await.unwrap();

        let new_docs = get_all(&registry, &new_reader_txn_id, "Users")
            .await
            .unwrap();
        assert_eq!(
            new_docs.len(),
            1,
            "New reader should see the committed data"
        );
        assert_eq!(
            new_docs[0].get("name").unwrap().as_str(),
            Some("CommittedData")
        );

        registry.rollback(&new_reader_txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_new_transaction_sees_recently_created_collection() {
    workers::block_on(async {
        // Test that a transaction started AFTER a collection is created can see that collection
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let registry = DbTransactionRegistry::new(db.clone());

        // Create collection after registry is created
        db.create_collection(CollectionVersion::new(
            "NewCollection",
            "v1",
            "col-new",
            vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())],
        ))
        .await
        .unwrap();

        // New transaction should see the collection
        let txn_id = registry.begin(true).await.unwrap();
        let collection_names = registry.collection_names().unwrap();
        assert!(
            collection_names.contains(&"NewCollection".to_string()),
            "New transaction should see recently created collection"
        );

        registry.rollback(&txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_collection_snapshot_isolation_during_deletion() {
    workers::block_on(async {
        // Test snapshot isolation: a transaction that started before a collection
        // is deleted should still be able to query that collection
        let db = test_db_with_collections().await;
        let registry = DbTransactionRegistry::new(db.clone());

        // Add some data to the collection first
        seed_committed_user(&db, "Alice", Some(30)).await;

        // Start a transaction BEFORE deletion
        let reader_txn_id = registry.begin(true).await.unwrap();

        // Verify reader can see the collection with data
        let docs_before = get_all(&registry, &reader_txn_id, "Users").await.unwrap();
        assert_eq!(
            docs_before.len(),
            1,
            "Should see 1 document before deletion"
        );

        // Now delete the collection from the DB
        db.delete_collection("Users").await.unwrap();

        // The reader transaction should STILL be able to query the collection
        // because it has a snapshot from before the deletion
        let docs_after = get_all(&registry, &reader_txn_id, "Users").await.unwrap();
        assert_eq!(
            docs_after.len(),
            1,
            "Reader should still see document after deletion due to snapshot isolation"
        );
        assert_eq!(
            docs_after[0].get("name").unwrap().as_str(),
            Some("Alice"),
            "Should see the same document content"
        );

        // However, the DB should report the collection as gone
        assert!(
            !db.has_collection("Users").unwrap(),
            "DB should report collection as deleted"
        );

        registry.rollback(&reader_txn_id).await.unwrap();

        // A NEW transaction should NOT see the deleted collection
        let new_txn_id = registry.begin(true).await.unwrap();

        let result = get_all(&registry, &new_txn_id, "Users").await;
        assert!(
            result.is_err(),
            "New transaction should not see deleted collection"
        );
        assert!(
            result.unwrap_err().to_string().contains("Users"),
            "Error should mention the collection name"
        );

        registry.rollback(&new_txn_id).await.unwrap();
    })
    .unwrap()
}

#[test]
fn test_collections_snapshot_is_isolated_from_modifications() {
    workers::block_on(async {
        // Test that modifying a snapshot does not affect the original cache
        let db = test_db_with_collections().await;

        let snapshot = db.collections_snapshot().unwrap();
        assert!(snapshot.contains("Users"));

        // The snapshot should be an independent copy
        // (This is implicitly tested by the fact that CollectionSnapshot
        // wraps an Arc and doesn't expose mutable methods)

        // Adding a new collection should not affect existing snapshots
        db.create_collection(CollectionVersion::new(
            "Posts",
            "v1",
            "col-posts",
            vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())],
        ))
        .await
        .unwrap();

        // Original snapshot should still only have Users
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.contains("Users"));
        assert!(!snapshot.contains("Posts"));

        // New snapshot should have both
        let new_snapshot = db.collections_snapshot().unwrap();
        assert_eq!(new_snapshot.len(), 2);
        assert!(new_snapshot.contains("Users"));
        assert!(new_snapshot.contains("Posts"));
    })
    .unwrap()
}
