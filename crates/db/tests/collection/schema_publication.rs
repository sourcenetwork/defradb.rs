use std::sync::Arc;

use db::DB;
use regolith::sync::Event;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::{Key, OnCommit};
use storage::keys::systemstore::CollectionNameKey;
use storage::RegolithStore;

#[test]
fn schema_success_waits_for_its_ordered_cache_publication() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let entered = Arc::new(Event::new());
        let release = Arc::new(Event::new());
        let earlier = db.new_txn(false).await.unwrap();
        earlier
            .systemstore()
            .unwrap()
            .set(b"earlier", b"commit")
            .await
            .unwrap();
        let (hook_entered, hook_release) = (Arc::clone(&entered), Arc::clone(&release));
        earlier
            .on_commit(OnCommit::OrderedAsync(Box::new(move || {
                Box::pin(async move {
                    hook_entered.set();
                    hook_release.wait().await;
                    Ok(())
                })
            })))
            .unwrap();

        let create = async {
            entered.wait().await;
            let schema = CollectionVersion::new(
                "Note",
                "note-v1",
                "note-id",
                vec![
                    FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                    FieldDescription::new("2", "text", FieldKind::string()),
                ],
            );
            let pending = db.create_collections_atomic(vec![schema]);
            futures::pin_mut!(pending);
            assert!(
                futures::poll!(&mut pending).is_pending(),
                "schema success preceded its ordered cache publication"
            );
            let reader = db.new_txn(true).await.unwrap();
            assert!(
                reader
                    .systemstore()
                    .unwrap()
                    .get(&CollectionNameKey::new("Note").bytes())
                    .await
                    .unwrap()
                    .is_some(),
                "the schema must already be durable while its publication waits"
            );
            assert!(db.query_schema("Note").is_none());
            release.set();
            pending.await.unwrap();
            assert!(db.query_schema("Note").is_some());
        };
        let (committed, ()) = futures::join!(earlier.commit(), create);
        committed.unwrap();
    })
    .unwrap();
}
