use std::sync::Arc;

use db::{AutoCommitFetcher, AutoCommitMutator, DB};
use document::{DocID, Document, NormalValue};
use query::{DocFetcher, DocMutator, Filter};
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::corekv::Key;
use storage::keys::DatastoreSE;
use storage::RegolithStore;

fn users_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
            FieldDescription::new("3", "age", FieldKind::int()),
        ],
    )
    .with_index(
        IndexDescription::new("users_name")
            .with_field("name", false)
            .as_unique(),
    )
}

fn age_filter(age: i64) -> Filter {
    let mut conditions = serde_json::Map::new();
    conditions.insert("age".into(), serde_json::json!({"_eq": age}));
    Filter::from_conditions(conditions)
}

async fn create_user(mutator: &AutoCommitMutator<RegolithStore>, name: &str, age: i64) -> DocID {
    let mut doc = Document::new();
    doc.set("name", NormalValue::String(name.to_owned()));
    doc.set("age", NormalValue::Int(age));
    mutator.create("Users", doc).await.unwrap().doc_id
}

#[tokio::test]
async fn filtered_truncate_removes_only_matching_documents_and_indexes() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let alice = create_user(&mutator, "Alice", 30).await;
    create_user(&mutator, "Bob", 40).await;

    db.truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap();

    let docs = AutoCommitFetcher::new(db.clone())
        .get_all("Users")
        .await
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0].get("name"),
        Some(&NormalValue::String("Bob".into()))
    );

    let recreated = create_user(&mutator, "Alice", 30).await;
    assert_eq!(recreated, alice);
    assert_eq!(
        AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn filtered_truncate_includes_soft_deleted_documents() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());

    let mut doc = Document::new();
    doc.set("name", NormalValue::String("Alice".into()));
    doc.set("age", NormalValue::Int(30));
    let created = mutator.create("Users", doc).await.unwrap();
    mutator.delete("Users", &created.doc_id).await.unwrap();

    db.truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap();
    create_user(&mutator, "Alice", 31).await;

    assert_eq!(
        AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .len(),
        1
    );
}

/// A p2p subscription in the systemstore marks the collection replicated:
/// peers walk its DAG, so pruning documents is still refused.
#[tokio::test]
async fn filtered_truncate_rejects_a_subscribed_branchable_collection() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema().as_branchable())
        .await
        .unwrap();
    subscribe_collection(&db, "users-collection").await;

    let error = db
        .truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        db::Error::FilteredTruncateBranchableCollection
    ));
}

/// A replicator entry naming the collection marks it replicated the same way.
#[tokio::test]
async fn filtered_truncate_rejects_a_replicated_branchable_collection() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema().as_branchable())
        .await
        .unwrap();
    add_replicator(&db, "users-collection").await;

    let error = db
        .truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        db::Error::FilteredTruncateBranchableCollection
    ));
}

/// Nothing replicates the collection, so the truncate goes through and the
/// collection DAG is pruned with the documents: surviving documents keep
/// their data but the head chain that referenced the removed ones is gone.
#[tokio::test]
async fn filtered_truncate_prunes_a_local_branchable_collection_and_its_dag() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema().as_branchable())
        .await
        .unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    create_user(&mutator, "Alice", 30).await;
    create_user(&mutator, "Bob", 40).await;

    db.truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap();

    let survivors = AutoCommitFetcher::new(db.clone())
        .get_all("Users")
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(
        survivors[0].get("name").unwrap(),
        &document::NormalValue::String("Bob".into())
    );

    let txn = db.new_txn(false).await.unwrap();
    let headstore = txn.headstore().unwrap();
    let short_id = db
        .get_collection("Users")
        .unwrap()
        .unwrap()
        .resolved_root_id();
    let prefix = storage::keys::headstore::HeadstoreColKey::collection_prefix(short_id);
    let mut iter = headstore
        .iterator(storage::corekv::IterOptions::new().with_prefix(prefix.clone()))
        .await
        .unwrap();
    let mut heads = 0;
    while iter.next().await.unwrap().is_some() {
        heads += 1;
    }
    iter.close().await.unwrap();
    drop(iter);
    drop(headstore);
    txn.commit().await.unwrap();
    assert_eq!(
        heads, 0,
        "the collection head chain is pruned with the documents"
    );
}

/// Writing the persisted p2p states directly, the way the p2p stack does.
async fn subscribe_collection(db: &DB<RegolithStore>, collection_id: &str) {
    let txn = db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    systemstore
        .set(
            &storage::keys::P2PCollectionKey::new(collection_id).bytes(),
            b"subscribed",
        )
        .await
        .unwrap();
    drop(systemstore);
    txn.commit().await.unwrap();
}

async fn add_replicator(db: &DB<RegolithStore>, collection_id: &str) {
    let txn = db.new_txn(false).await.unwrap();
    let peerstore = txn.peerstore().unwrap();
    let entry = format!("{{\"ID\":\"peer-1\",\"CollectionIDs\":[\"{collection_id}\"]}}");
    peerstore
        .set(
            &storage::keys::ReplicatorKey::new("peer-1").bytes(),
            entry.as_bytes(),
        )
        .await
        .unwrap();
    drop(peerstore);
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn filtered_truncate_removes_only_matching_searchable_encryption_records() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let alice = create_user(&mutator, "Alice", 30).await;
    let bob = create_user(&mutator, "Bob", 40).await;
    let alice_key =
        DatastoreSE::new("users-collection", "users-name", vec![1], alice.to_string()).bytes();
    let bob_key =
        DatastoreSE::new("users-collection", "users-name", vec![2], bob.to_string()).bytes();

    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    datastore.set(&alice_key, b"alice").await.unwrap();
    datastore.set(&bob_key, b"bob").await.unwrap();
    drop(datastore);
    txn.commit().await.unwrap();

    db.truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap();

    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    assert!(datastore.get(&alice_key).await.unwrap().is_none());
    assert!(datastore.get(&bob_key).await.unwrap().is_some());
}

#[tokio::test]
async fn filtered_truncate_processes_more_than_one_chunk() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let schema = CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "age", FieldKind::int()),
            FieldDescription::new("3", "seq", FieldKind::int()),
        ],
    );
    db.create_collection(schema).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    for seq in 0..=1000 {
        let mut doc = Document::new();
        doc.set("age", NormalValue::Int(30));
        doc.set("seq", NormalValue::Int(seq));
        mutator.create("Users", doc).await.unwrap();
    }

    db.truncate_collection_with_filter("Users", age_filter(30), None)
        .await
        .unwrap();

    assert!(AutoCommitFetcher::new(db)
        .get_all("Users")
        .await
        .unwrap()
        .is_empty());
}
