//! A document's version key records the schema version its value was written
//! at. A create writes it; an update at the same version leaves it alone, so
//! an update never rewrites the key with the bytes it already holds.

use std::sync::Arc;

use db::{DbDocMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::namespace::Namespace;
use storage::RegolithStore;

use crate::common::counting_store::CountingStore;

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        "Versioned",
        "versioned-v1",
        "col-versioned",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "left", FieldKind::int(), schema::CType::LwwRegister),
            FieldDescription::new("3", "right", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
}

fn is_version_key(key: &[u8]) -> bool {
    key.first() == Some(&Namespace::Datastore.prefix()) && key[1..].starts_with(b"/v/")
}

#[test]
fn an_update_at_the_same_version_does_not_rewrite_the_version_key() {
    workers::block_on(async {
        let db =
            Arc::new(DB::new(CountingStore::new(RegolithStore::in_memory().unwrap())).unwrap());
        db.create_collection(collection()).await.unwrap();

        let creator = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let created = creator
            .create(
                "Versioned",
                Document::from_json_str(r#"{"left": 0, "right": 0}"#, collection()).unwrap(),
            )
            .await
            .unwrap();
        creator.take_txn().await.unwrap().commit().await.unwrap();
        assert!(
            db.store()
                .take_written_keys()
                .iter()
                .any(|key| is_version_key(key)),
            "a create writes its version key"
        );

        let updater = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let mut doc = updater
            .get_for_update("Versioned", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("left", NormalValue::Int(1)).unwrap();
        updater
            .update(
                "Versioned",
                doc,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .unwrap();
        updater.take_txn().await.unwrap().commit().await.unwrap();

        let written = db.store().take_written_keys();
        assert!(!written.is_empty(), "the update wrote its document");
        assert!(
            !written.iter().any(|key| is_version_key(key)),
            "an update at the same version rewrote the version key"
        );
    })
    .unwrap()
}
