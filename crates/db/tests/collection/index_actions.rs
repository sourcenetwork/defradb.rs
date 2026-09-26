use std::sync::Arc;

use db::{DbCollectionProvider, DB};
use defra_core::{Action, ActionStatus};
use query::fetcher::CollectionProvider;
use query::txn::TransactionRegistry;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::{corekv::Key, keys::systemstore::ActionStatusKey, RegolithStore};

#[tokio::test]
async fn recaching_and_activation_preserve_pending_index_restrictions() {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(
        CollectionVersion::new(
            "Note",
            "v1",
            "notes",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "title", FieldKind::string()),
            ],
        )
        .with_index(IndexDescription::new("by_title").with_field("title", false)),
    )
    .await
    .unwrap();
    let schema = db.get_collection("Note").unwrap().unwrap().schema().clone();
    let provider = DbCollectionProvider::new(db.clone());
    assert_eq!(
        provider
            .get_collection("Note")
            .await
            .unwrap()
            .unwrap()
            .indexes
            .len(),
        1
    );
    for status in [ActionStatus::IN_PROGRESS, ActionStatus::ERRORED] {
        let txn = db.new_txn(false).await.unwrap();
        txn.systemstore()
            .unwrap()
            .set(
                &ActionStatusKey::with_subject(
                    &schema.collection_id,
                    Action::BACKFILL_INDEX,
                    schema.indexes[0].id.to_string(),
                )
                .bytes(),
                &db::database::action::encode_status(status),
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_eq!(
            db.add_collection_to_cache(schema.clone()).await.unwrap(),
            db::Cached::Taken
        );
        assert!(provider
            .get_collection("Note")
            .await
            .unwrap()
            .unwrap()
            .indexes
            .is_empty());
        assert!(provider
            .get_collection_by_version_id(&schema.version_id)
            .await
            .unwrap()
            .unwrap()
            .indexes
            .is_empty());
        db.set_active_collection_version(&schema.version_id)
            .await
            .unwrap();
        assert!(provider
            .get_collection("Note")
            .await
            .unwrap()
            .unwrap()
            .indexes
            .is_empty());
        let registry = db::txn::registry::DbTransactionRegistry::new(db.clone());
        let handle = registry.begin(false).await.unwrap();
        registry
            .set_collection_active_in_txn(handle.as_str(), &schema.version_id, true)
            .await
            .unwrap();
        registry.commit(&handle).await.unwrap();
        assert!(provider
            .get_collection("Note")
            .await
            .unwrap()
            .unwrap()
            .indexes
            .is_empty());
    }
    let patched = db
        .patch_collection(
            "Note",
            r#"[{"op":"add","path":"/Note/Fields/-","value":{"Name":"extra","Kind":"String"}}]"#,
            None,
        )
        .await
        .unwrap();
    assert_eq!(patched.indexes.len(), 1);
    assert!(provider
        .get_collection("Note")
        .await
        .unwrap()
        .unwrap()
        .indexes
        .is_empty());
}
