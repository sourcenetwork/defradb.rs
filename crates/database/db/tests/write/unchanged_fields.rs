//! An update that writes a field's current value adds no block for it, and an
//! update that changes nothing writes nothing at all (#1986).

use db::AutoCommitMutator;
use db::DbDocMutator;
use db::DB;
use document::Document;
use document::NormalValue;
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use std::sync::Arc;
use storage::corekv::IterOptions;
use storage::RegolithStore;

fn status_collection() -> CollectionVersion {
    CollectionVersion::new(
        "Status",
        "v1",
        "col-status",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "state",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "3",
                "detail",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
        ],
    )
}

fn fields(names: &[&str]) -> RapidHashSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

async fn block_count(db: &DB<RegolithStore>) -> usize {
    let txn = db.new_txn(true).await.unwrap();
    let blockstore = txn.blockstore().unwrap();
    let mut iter = blockstore.iterator(IterOptions::new()).await.unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    iter.close().await.unwrap();
    count
}

async fn setup() -> (Arc<DB<RegolithStore>>, document::DocID) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(status_collection()).await.unwrap();
    let created = AutoCommitMutator::new(Arc::clone(&db))
        .create(
            "Status",
            Document::from_json_str(
                r#"{"state": "idle", "detail": "boot"}"#,
                status_collection(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    (db, created.doc_id)
}

#[test]
fn autocommit_update_with_current_values_writes_nothing() {
    workers::block_on(async {
        let (db, doc_id) = setup().await;
        let auto = AutoCommitMutator::new(Arc::clone(&db));
        let before = block_count(&db).await;

        let doc = auto
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        let result = auto
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();

        assert_eq!(result.fields_modified, 0);
        assert!(result.commit_cid.is_none());
        assert_eq!(block_count(&db).await, before);
    })
    .unwrap()
}

#[test]
fn autocommit_update_writes_only_the_changed_field() {
    workers::block_on(async {
        let (db, doc_id) = setup().await;
        let auto = AutoCommitMutator::new(Arc::clone(&db));
        let before = block_count(&db).await;

        let mut doc = auto
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("state", NormalValue::String("busy".to_string()))
            .unwrap();
        let result = auto
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();

        assert!(result.commit_cid.is_some());
        // One field block for `state` and one composite.
        assert_eq!(block_count(&db).await, before + 2);
        assert_eq!(
            result.document.get("detail"),
            Some(&NormalValue::String("boot".to_string()))
        );
    })
    .unwrap()
}

#[test]
fn transaction_update_with_current_values_writes_nothing() {
    workers::block_on(async {
        let (db, doc_id) = setup().await;
        let before = block_count(&db).await;

        let mutator = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let doc = mutator
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        let result = mutator
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();
        mutator.take_txn().await.unwrap().commit().await.unwrap();

        assert_eq!(result.fields_modified, 0);
        assert_eq!(block_count(&db).await, before);
    })
    .unwrap()
}

#[test]
fn transaction_update_writes_only_the_changed_field() {
    workers::block_on(async {
        let (db, doc_id) = setup().await;
        let before = block_count(&db).await;

        let mutator = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let mut doc = mutator
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("state", NormalValue::String("busy".to_string()))
            .unwrap();
        let result = mutator
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();
        mutator.take_txn().await.unwrap().commit().await.unwrap();

        assert_eq!(result.fields_modified, 1);
        assert_eq!(block_count(&db).await, before + 2);
    })
    .unwrap()
}

#[test]
fn batch_update_with_current_values_writes_nothing() {
    workers::block_on(async {
        use db::write::autocommit::batch::BatchMutator;
        use query::mutator::MutationBatchController;

        let (db, doc_id) = setup().await;
        let before = block_count(&db).await;
        let mutator =
            BatchMutator::new(Arc::clone(&db), Arc::new(db.new_txn(false).await.unwrap())).await;
        let mut doc = mutator
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set_write_preparation(Arc::new(document::WritePreparation::new(1, 1)));
        let result = mutator
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();
        mutator.commit().await.unwrap();

        assert_eq!(result.fields_modified, 0);
        assert!(result.commit_cid.is_none());
        assert!(result.document.write_preparation().is_none());
        assert_eq!(block_count(&db).await, before);
    })
    .unwrap()
}

#[test]
fn batch_update_writes_only_the_changed_field() {
    workers::block_on(async {
        use db::write::autocommit::batch::BatchMutator;
        use query::mutator::MutationBatchController;

        let (db, doc_id) = setup().await;
        let before = block_count(&db).await;
        let mutator =
            BatchMutator::new(Arc::clone(&db), Arc::new(db.new_txn(false).await.unwrap())).await;
        let mut doc = mutator
            .get_for_update("Status", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("state", NormalValue::String("busy".into()))
            .unwrap();
        let result = mutator
            .update("Status", doc, fields(&["state", "detail"]))
            .await
            .unwrap();
        mutator.commit().await.unwrap();

        assert_eq!(result.fields_modified, 1);
        assert_eq!(block_count(&db).await, before + 2);
        assert_eq!(
            result.document.get("detail"),
            Some(&NormalValue::String("boot".into()))
        );
    })
    .unwrap()
}
