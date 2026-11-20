use std::sync::Arc;

use db::{AutoCommitFetcher, AutoCommitMutator, TruncateError, DB};
use defra_core::{RequestContext, TxnId};
use document::{DocID, Document, NormalValue};
use query::documents::Filter;
use query::{DocFetcher, DocMutator};
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::corekv::{IterOptions, Key};
use storage::keys::DatastoreSE;
use storage::RegolithStore;

fn users_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            FieldDescription::new("3", "age", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
    .with_index(
        IndexDescription::new("users_name")
            .with_field("name", false)
            .as_unique(),
    )
}

fn age_filter(age: i64) -> Filter {
    serde_json::json!({"age": {"_eq": age}}).into()
}

async fn create_user(mutator: &AutoCommitMutator<RegolithStore>, name: &str, age: i64) -> DocID {
    let mut doc = Document::new(users_schema());
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    doc.set("age", NormalValue::Int(age)).unwrap();
    mutator.create("Users", doc).await.unwrap().doc_id
}

#[test]
fn filtered_truncate_includes_soft_deleted_documents() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mutator = AutoCommitMutator::new(db.clone());

        let mut doc = Document::new(users_schema());
        doc.set("name", NormalValue::String("Alice".into()))
            .unwrap();
        doc.set("age", NormalValue::Int(30)).unwrap();
        let created = mutator.create("Users", doc).await.unwrap();
        mutator.delete("Users", &created.doc_id).await.unwrap();

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
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
    })
    .unwrap()
}

#[test]
fn filtered_truncate_rejects_branchable_collections() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema().as_branchable())
            .await
            .unwrap();

        let error = db
            .truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap_err();

        assert!(matches!(error, TruncateError::FilteredBranchable));
    })
    .unwrap()
}

#[test]
fn filtered_truncate_removes_only_matching_searchable_encryption_records() {
    workers::block_on(async {
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

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let datastore = txn.datastore().unwrap();
        assert!(datastore.get(&alice_key).await.unwrap().is_none());
        assert!(datastore.get(&bob_key).await.unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn filtered_truncate_processes_more_than_one_chunk() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let schema = CollectionVersion::new(
            "Users",
            "users-v1",
            "users-collection",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "age", FieldKind::int(), schema::CType::LwwRegister),
                FieldDescription::new("3", "seq", FieldKind::int(), schema::CType::LwwRegister),
            ],
        );
        db.create_collection(schema.clone()).await.unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        for seq in 0..=1000 {
            let mut doc = Document::new(schema.clone());
            doc.set("age", NormalValue::Int(30)).unwrap();
            doc.set("seq", NormalValue::Int(seq)).unwrap();
            mutator.create("Users", doc).await.unwrap();
        }

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();

        assert!(AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn filtered_truncate_in_a_transaction_is_refused_and_removes_nothing() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;
        let in_txn = RequestContext::ambient().with_txn(Some(TxnId(1)));

        let error = db
            .truncate_collection_as(&in_txn, "Users", Some(&age_filter(30)))
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "filtered truncate cannot run in a transaction"
        );
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

#[test]
fn unfiltered_truncate_in_a_transaction_applies_at_once() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;
        let in_txn = RequestContext::ambient().with_txn(Some(TxnId(1)));

        db.truncate_collection_as(&in_txn, "Users", None)
            .await
            .unwrap();

        assert!(AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn a_filter_that_is_not_one_is_refused_with_go_text() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;

        for (filter, expected) in [
            (Filter::Source("  ".into()), "filter cannot be empty"),
            (
                Filter::from(serde_json::json!(4)),
                "unsupported filter type",
            ),
            (
                Filter::from(serde_json::json!(null)),
                "unsupported filter type",
            ),
        ] {
            let error = db
                .truncate_collection_as(&RequestContext::ambient(), "Users", Some(&filter))
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), expected);
        }
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

#[test]
fn a_source_text_filter_truncates_what_it_matches() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        create_user(&mutator, "Alice", 30).await;
        create_user(&mutator, "Bob", 40).await;

        db.truncate_collection_as(
            &RequestContext::ambient(),
            "Users",
            Some(&Filter::Source("{age: {_eq: 30}}".into())),
        )
        .await
        .unwrap();

        let docs = AutoCommitFetcher::new(db).get_all("Users").await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("name"),
            Some(&NormalValue::String("Bob".into()))
        );
    })
    .unwrap()
}

#[test]
fn truncating_a_missing_collection_names_it() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());

        let error = db
            .truncate_collection_as(&RequestContext::ambient(), "Users", None)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TruncateError::Db(db::Error::CollectionNotFound(name)) if name == "Users"
        ));
    })
    .unwrap()
}

fn named_schema(name: &str) -> CollectionVersion {
    CollectionVersion::new(
        name,
        format!("{name}-v1"),
        format!("{name}-collection"),
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
    .with_index(IndexDescription::new(format!("{name}_name")).with_field("name", false))
}

async fn create_named(db: &Arc<DB<RegolithStore>>, collection: &str) {
    let mut doc = Document::new(named_schema(collection));
    doc.set("name", NormalValue::String(collection.to_owned()))
        .unwrap();
    AutoCommitMutator::new(db.clone())
        .create(collection, doc)
        .await
        .unwrap();
}

async fn datastore_keys(db: &Arc<DB<RegolithStore>>, prefix: &str) -> usize {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_prefix(prefix.as_bytes().to_vec()))
        .await
        .unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    iter.close().await.unwrap();
    count
}

/// A collection whose short ID is written as the byte of a family other
/// collections keep their keys in (100 is `d`, the documents; 118 is `v`,
/// their schema versions) is truncated without touching those keys.
#[test]
fn truncating_a_collection_whose_short_id_spells_a_family_keeps_other_collections() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(named_schema("Kept")).await.unwrap();
        create_named(&db, "Kept").await;
        let mut spelled = Vec::new();
        for n in 0..200 {
            let name = format!("Filler{n}");
            db.create_collection(named_schema(&name)).await.unwrap();
            let short_id = db
                .get_collection(&name)
                .unwrap()
                .unwrap()
                .resolved_root_id();
            if short_id == u32::from(b'd') || short_id == u32::from(b'v') {
                create_named(&db, &name).await;
                spelled.push(name);
            }
            if short_id > u32::from(b'v') {
                break;
            }
        }
        assert_eq!(spelled.len(), 2, "no collection got short ID 100 or 118");
        let kept_documents = datastore_keys(&db, "/d/Kept-collection/").await;
        let kept_versions = datastore_keys(&db, "/v/Kept-collection/").await;
        assert!(kept_documents > 0 && kept_versions > 0);

        for name in &spelled {
            db.truncate_collection_as(&RequestContext::ambient(), name, None)
                .await
                .unwrap();
            assert!(AutoCommitFetcher::new(db.clone())
                .get_all(name)
                .await
                .unwrap()
                .is_empty());
        }

        assert_eq!(
            datastore_keys(&db, "/d/Kept-collection/").await,
            kept_documents
        );
        assert_eq!(
            datastore_keys(&db, "/v/Kept-collection/").await,
            kept_versions
        );
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Kept")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

/// Truncate removes a document's DocID mapping and its blocks' ownership:
/// before it the genesis field block is owned by the DocID, after it the
/// DocID resolves to nothing and the block has no owner.
#[test]
#[mantra_macros::req_test("defra.collection.truncate_ownership")]
fn truncate_removes_doc_id_mappings_and_block_ownership() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mut doc = Document::new(users_schema());
        doc.set("name", NormalValue::String("Alice".to_owned()))
            .unwrap();
        doc.set("age", NormalValue::Int(30)).unwrap();
        let created = AutoCommitMutator::new(db.clone())
            .create("Users", doc)
            .await
            .unwrap();
        let composite =
            defra_core::Block::from_dag_cbor(created.commit_block.as_ref().unwrap()).unwrap();
        let field = composite.links.unwrap()[0].link.to_string();
        let doc_id = created.doc_id.to_string();

        let lookups = || {
            let db = db.clone();
            let (field, doc_id) = (field.clone(), doc_id.clone());
            async move {
                let txn = db.new_txn(true).await.unwrap();
                let systemstore = txn.systemstore().unwrap();
                let owners = db::docid::map::get_doc_ids_for_block(&systemstore, &field)
                    .await
                    .unwrap();
                let resolves = db::docid::map::get_doc_ref(&systemstore, &doc_id)
                    .await
                    .unwrap()
                    .is_some();
                drop(systemstore);
                let _ = txn.discard();
                (owners, resolves)
            }
        };
        assert_eq!(lookups().await, (vec![doc_id.clone()], true));
        db.truncate_collection_as(&RequestContext::ambient(), "Users", None)
            .await
            .unwrap();
        assert_eq!(lookups().await, (Vec::<String>::new(), false));
    })
    .unwrap()
}
