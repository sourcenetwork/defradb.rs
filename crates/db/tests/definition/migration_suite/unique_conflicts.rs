use super::*;

async fn unique_migration_db() -> (Arc<DB<RegolithStore>>, String) {
    unique_migration_db_with_value(None).await
}

async fn unique_migration_db_with_value(
    verified_value: Option<serde_json::Value>,
) -> (Arc<DB<RegolithStore>>, String) {
    let is_array = verified_value
        .as_ref()
        .is_some_and(serde_json::Value::is_array);
    let mut raw_db = DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap();
    raw_db.set_lens_store(Arc::new(SetVerifiedStore {
        verified_value,
        ..Default::default()
    }));
    let db = Arc::new(raw_db);
    let mut schema = indexed_users_schema();
    schema.indexes[0].unique = true;
    if is_array {
        schema.fields[2].kind = FieldKind::bool_array();
    }
    db.create_collection(schema).await.unwrap();
    let v1 = db
        .get_collection("Users")
        .unwrap()
        .unwrap()
        .version_id()
        .to_string();
    let v2 = add_placeholder_version(&db, "placeholder").await;
    db.set_migration(
        LensConfig::new(
            &v1,
            &v2,
            LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
        ),
        None,
    )
    .await
    .unwrap();
    (db, v1)
}

async fn stored_entries(db: &DB<RegolithStore>) -> Vec<(Vec<u8>, Vec<u8>)> {
    let txn = db.new_txn(true).await.unwrap();
    let mut entries = Vec::new();
    for store in [txn.datastore().unwrap(), txn.systemstore().unwrap()] {
        let mut iter = store.iterator(IterOptions::new()).await.unwrap();
        while let Some(entry) = iter.next().await.unwrap() {
            entries.push((entry.key.to_vec(), entry.value.to_vec()));
        }
        iter.close().await.unwrap();
    }
    txn.discard().unwrap();
    entries
}

#[tokio::test]
async fn materialize_rejects_new_unique_collision_and_rolls_back() {
    let (db, v1) = unique_migration_db().await;
    seed_old_user(&db, &v1, "Alice").await;
    seed_old_user(&db, &v1, "Bob").await;
    let before = stored_entries(&db).await;

    let err = db.materialize_collection("Users").await.unwrap_err();
    assert!(err.is_unique_constraint_violation(), "{err}");
    assert_eq!(stored_entries(&db).await, before);
}

async fn set_stored_verified(db: &DB<RegolithStore>, doc_id: &document::DocID, current: bool) {
    set_stored_value(db, doc_id, current, document::NormalValue::Bool(true)).await;
}

async fn set_stored_value(
    db: &DB<RegolithStore>,
    doc_id: &document::DocID,
    current: bool,
    value: document::NormalValue,
) {
    let collection = db.get_collection("Users").unwrap().unwrap();
    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = collection
        .resolve_doc_short_id(&systemstore, doc_id)
        .await
        .unwrap()
        .unwrap();
    let mut doc = collection
        .get_by_doc_id(&datastore, &systemstore, doc_id)
        .await
        .unwrap()
        .unwrap();
    doc.set("verified", value);
    if current {
        doc.set_schema_version_id(collection.version_id());
    }
    collection
        .save_with_datastore(&datastore, &doc, short_id)
        .await
        .unwrap();
    drop(datastore);
    drop(systemstore);
    txn.commit().await.unwrap();
}

#[tokio::test]
async fn materialize_rejects_collision_with_current_document_in_either_order() {
    for current_first in [false, true] {
        let (db, v1) = unique_migration_db().await;
        let first = seed_old_user(&db, &v1, "Alice").await;
        let second = seed_old_user(&db, &v1, "Bob").await;
        set_stored_verified(&db, if current_first { &first } else { &second }, true).await;
        let before = stored_entries(&db).await;

        let err = db
            .reindex_collection_with_migrations("Users")
            .await
            .unwrap_err();
        assert!(err.is_unique_constraint_violation(), "{err}");
        assert_eq!(stored_entries(&db).await, before);
    }
}

#[tokio::test]
async fn materialize_preserves_preexisting_unique_collision_across_transform() {
    let (db, v1) = unique_migration_db().await;
    let first = seed_old_user(&db, &v1, "Alice").await;
    let second = seed_old_user(&db, &v1, "Bob").await;
    // Seed the duplicate persisted values that replication can legitimately retain.
    set_stored_verified(&db, &first, false).await;
    set_stored_verified(&db, &second, false).await;

    assert_eq!(db.materialize_collection("Users").await.unwrap(), 2);
    assert_eq!(verified_index_count(&db).await, 1);
    let collection = db.get_collection("Users").unwrap().unwrap();
    for id in [&first, &second] {
        let doc = load_user(&db, id).await;
        assert_eq!(doc.schema_version_id(), Some(collection.version_id()));
        assert_eq!(
            doc.get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
    }
    let txn = db.new_txn(true).await.unwrap();
    let manager = db::index::IndexManager::from_collection(
        collection.resolved_root_id(),
        collection.schema(),
    )
    .unwrap();
    let mut iter = manager
        .get_index("idx_verified")
        .unwrap()
        .get(
            &txn.datastore().unwrap(),
            &[document::NormalValue::Bool(true)],
        )
        .await
        .unwrap();
    let entries = iter.collect_all().await.unwrap();
    let expected = if first.to_string() < second.to_string() {
        &first
    } else {
        &second
    };
    let short_id = collection
        .resolve_doc_short_id(&txn.systemstore().unwrap(), expected)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entries[0].doc_short_id, short_id);
    drop(iter);
    txn.discard().unwrap();
}

#[tokio::test]
async fn materialize_checks_array_conflicts_per_value() {
    for introduces_collision in [false, true] {
        let output = if introduces_collision {
            serde_json::json!([true, false])
        } else {
            serde_json::json!([true])
        };
        let (db, v1) = unique_migration_db_with_value(Some(output)).await;
        let first = seed_old_user(&db, &v1, "Alice").await;
        let second = seed_old_user(&db, &v1, "Bob").await;
        set_stored_value(
            &db,
            &first,
            false,
            document::NormalValue::BoolArray(vec![true]),
        )
        .await;
        set_stored_value(
            &db,
            &second,
            false,
            document::NormalValue::BoolArray(vec![true, false]),
        )
        .await;
        let before = stored_entries(&db).await;

        let result = db.materialize_collection("Users").await;
        if introduces_collision {
            let err = result.unwrap_err();
            assert!(err.is_unique_constraint_violation(), "{err}");
            assert_eq!(stored_entries(&db).await, before);
        } else {
            assert_eq!(result.unwrap(), 2);
            assert_eq!(verified_index_count(&db).await, 1);
        }
    }
}
