//! `DB::drop_index`: the definition, the backfill record and the drop record
//! change in one commit, the entries go in pages after it, and a restart
//! finishes what a drop or a backfill left behind.

use std::sync::Arc;

use db::index::backfill::encode_progress;
use db::index::drop::DropProgress;
use db::{AutoCommitMutator, BackfillSource, Collection, IndexManager, DB};
use defra_core::{Action, ActionStatus};
use schema::{CollectionVersion, IndexDescription};
use storage::corekv::Key;
use storage::keys::systemstore::{
    ActionProgressKey, ActionReasonKey, ActionStatusKey, CollectionKey, CollectionNameKey,
};
use storage::RegolithStore;

use super::create::{by_name, create_user, index_entries, open_with_collection};

const COLLECTION: &str = "Dropped";
const COLLECTION_ID: &str = "col-index-dropped";

async fn actions_for(db: &DB<RegolithStore>, action: Action) -> Vec<defra_core::ActionExecution> {
    db.list_actions()
        .await
        .unwrap()
        .into_iter()
        .filter(|execution| execution.action == action)
        .collect()
}

async fn write_definition(db: &DB<RegolithStore>, schema: &CollectionVersion) {
    let txn = db.new_txn(false).await.unwrap();
    {
        let systemstore = txn.systemstore().unwrap();
        systemstore
            .set(
                &CollectionKey::new(&schema.version_id).bytes(),
                &serde_json::to_vec(schema).unwrap(),
            )
            .await
            .unwrap();
        systemstore
            .set(
                &CollectionNameKey::new(COLLECTION).bytes(),
                schema.version_id.as_bytes(),
            )
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
}

/// A collection of `total` documents with `by_name` defined and its backfill
/// stopped after `indexed` documents, as a crash would leave it.
async fn half_backfilled(
    total: usize,
    indexed: usize,
) -> (
    Arc<RegolithStore>,
    Arc<DB<RegolithStore>>,
    CollectionVersion,
    IndexDescription,
) {
    let (store, db) = open_with_collection(COLLECTION, "dropped-v1", COLLECTION_ID).await;
    let mutator = AutoCommitMutator::new(db.clone());
    for i in 0..total {
        create_user(&mutator, COLLECTION, &format!("user-{i}")).await;
    }
    drop(mutator);

    let mut schema = db.require_collection(COLLECTION).unwrap().schema().clone();
    let (fields, kind) = by_name();
    let txn = db.new_txn(false).await.unwrap();
    let index = {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        let mut manager =
            IndexManager::from_collection(schema.resolved_root_id(), &schema).unwrap();
        let index = manager
            .create_index_of_kind(
                &datastore,
                COLLECTION,
                "by_name".to_string(),
                fields,
                kind,
                &schema.fields,
            )
            .await
            .unwrap();
        schema.indexes.push(index.clone());
        drop(
            db.stage_action(
                &systemstore,
                COLLECTION_ID,
                Action::BACKFILL_INDEX,
                &index.id.to_string(),
            )
            .await
            .unwrap(),
        );
        index
    };
    txn.commit().await.unwrap();
    write_definition(&db, &schema).await;

    let fence = db.next_doc_short_id().await.unwrap();
    let manager = IndexManager::from_collection(schema.resolved_root_id(), &schema).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        let mut source = BackfillSource::open_range(
            Collection::new(schema.clone()),
            datastore.clone(),
            systemstore.clone(),
            None,
            Some(fence),
        )
        .await
        .unwrap();
        let batch = manager
            .index_batch_from(&datastore, "by_name", &mut source, &schema, indexed)
            .await
            .unwrap();
        systemstore
            .set(
                &ActionProgressKey::new(
                    COLLECTION_ID,
                    Action::BACKFILL_INDEX,
                    index.id.to_string(),
                )
                .bytes(),
                &encode_progress(fence, batch.last_doc_short_id.unwrap()),
            )
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    db.reload_cache().await.unwrap();
    (store, db, schema, index)
}

#[test]
fn drop_index_clears_an_unfinished_backfill() {
    workers::block_on(async {
        let (store, db, _, index) = half_backfilled(300, 100).await;
        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 100);
        assert_eq!(actions_for(&db, Action::BACKFILL_INDEX).await.len(), 1);

        db.drop_index(COLLECTION, "by_name").await.unwrap();

        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 0);
        assert!(db.list_actions().await.unwrap().is_empty());
        assert!(db
            .require_collection(COLLECTION)
            .unwrap()
            .get_index("by_name")
            .is_none());
        drop(db);

        // Nothing is left for a restart to resume.
        let db = DB::open_from_arc(store).await.unwrap();
        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 0);
        assert!(db.list_actions().await.unwrap().is_empty());
        assert!(db
            .require_collection(COLLECTION)
            .unwrap()
            .get_index("by_name")
            .is_none());
    })
    .unwrap()
}

#[test]
fn drop_index_reports_a_missing_index() {
    workers::block_on(async {
        let (_store, db) = open_with_collection(COLLECTION, "dropped-v1", COLLECTION_ID).await;
        let error = db.drop_index(COLLECTION, "by_name").await.unwrap_err();
        assert!(
            matches!(error, db::Error::IndexNotFound { ref index, .. } if index == "by_name"),
            "{error}"
        );
    })
    .unwrap()
}

#[test]
fn a_drop_during_a_running_backfill_leaves_no_action() {
    workers::block_on(async {
        let (_store, db) = open_with_collection(COLLECTION, "dropped-v1", COLLECTION_ID).await;
        let mutator = AutoCommitMutator::new(db.clone());
        for i in 0..2_000 {
            create_user(&mutator, COLLECTION, &format!("user-{i}")).await;
        }
        drop(mutator);

        let (fields, kind) = by_name();
        let create = db.create_index(COLLECTION, Some("by_name"), fields, kind);
        let drop_while_running = async {
            while actions_for(&db, Action::BACKFILL_INDEX).await.is_empty() {
                workers::yield_now().await;
            }
            db.drop_index(COLLECTION, "by_name").await
        };
        let (created, dropped) = futures::join!(create, drop_while_running);
        let index = created.unwrap();
        dropped.unwrap();

        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 0);
        assert!(
            db.list_actions().await.unwrap().is_empty(),
            "{:?}",
            db.list_actions().await.unwrap()
        );
    })
    .unwrap()
}

#[test]
fn a_restart_finishes_an_unfinished_drop() {
    workers::block_on(async {
        let (store, db, mut schema, index) = half_backfilled(300, 300).await;
        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 300);

        // The drop's commit landed and the process stopped before any page.
        schema.indexes.retain(|existing| existing.id != index.id);
        write_definition(&db, &schema).await;
        let subject = index.id.to_string();
        let txn = db.new_txn(false).await.unwrap();
        {
            let systemstore = txn.systemstore().unwrap();
            db.clear_action(COLLECTION_ID, Action::BACKFILL_INDEX, &subject)
                .await
                .unwrap();
            drop(
                db.stage_action(&systemstore, COLLECTION_ID, Action::DROP_INDEX, &subject)
                    .await
                    .unwrap(),
            );
            systemstore
                .set(
                    &ActionProgressKey::new(COLLECTION_ID, Action::DROP_INDEX, &subject).bytes(),
                    &DropProgress::new(schema.resolved_root_id(), index.clone(), None)
                        .encode()
                        .unwrap(),
                )
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
        drop(db);

        let db = DB::open_from_arc(store).await.unwrap();
        assert_eq!(index_entries(&db, COLLECTION, index.id).await, 0);
        assert!(db.list_actions().await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_restart_clears_a_failed_backfill_of_a_dropped_index() {
    workers::block_on(async {
        let (store, db, mut schema, index) = half_backfilled(10, 10).await;

        // What a drop used to leave: the index gone, its failure recorded.
        schema.indexes.retain(|existing| existing.id != index.id);
        write_definition(&db, &schema).await;
        let subject = index.id.to_string();
        let txn = db.new_txn(false).await.unwrap();
        {
            let systemstore = txn.systemstore().unwrap();
            systemstore
                .set(
                    &ActionStatusKey::with_subject(COLLECTION_ID, Action::BACKFILL_INDEX, &subject)
                        .bytes(),
                    &db::database::action::encode_status(ActionStatus::ERRORED),
                )
                .await
                .unwrap();
            systemstore
                .set(
                    &ActionReasonKey::new(COLLECTION_ID, Action::BACKFILL_INDEX, &subject).bytes(),
                    b"index 'by_name' changed during backfill",
                )
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
        db.reload_cache().await.unwrap();
        assert_eq!(actions_for(&db, Action::BACKFILL_INDEX).await.len(), 1);
        drop(db);

        let db = DB::open_from_arc(store).await.unwrap();
        assert!(db.list_actions().await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_dropped_index_id_is_never_handed_out_again() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        // An index from the SDL numbers its id from a counter of its own,
        // which the id of an index created later used not to look past.
        for schema in query::parse_sdl("type Reused { name: String @index }").unwrap() {
            db.create_collection(schema).await.unwrap();
        }
        let dropped = db.require_collection("Reused").unwrap().schema().indexes[0].clone();
        db.drop_index("Reused", &dropped.name).await.unwrap();

        let (fields, kind) = by_name();
        let created = db
            .create_index("Reused", Some("by_name"), fields, kind)
            .await
            .unwrap();
        assert!(
            created.id > dropped.id,
            "dropped id {} came back as {}",
            dropped.id,
            created.id
        );
    })
    .unwrap()
}
