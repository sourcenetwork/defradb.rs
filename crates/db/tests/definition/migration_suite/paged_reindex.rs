use super::*;
use db::definition::migration::ReindexReport;
use db::index::backfill::BACKFILL_BATCH_DOCS;

/// More documents than two pages hold, so a rebuild spans pages.
const DOCS: usize = 2 * BACKFILL_BATCH_DOCS + 10;

#[test]
fn a_rebuild_that_stops_part_way_stays_owed_and_resumes_after_its_last_page() {
    workers::block_on(async {
        let lens = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(lens.clone());
        let db = Arc::new(raw_db);
        db.create_collection(indexed_users_schema()).await.unwrap();
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
        for user in 0..DOCS {
            seed_old_user(&db, &v1, &format!("User{user}")).await;
        }
        let collection_id = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .collection_id()
            .to_string();

        // The lens fails in the second page that walks documents, once the pages
        // that cleared the entries and walked the first documents committed.
        let before = lens.transform_calls.load(Ordering::SeqCst);
        lens.fail_call
            .store(before + BACKFILL_BATCH_DOCS + 5, Ordering::SeqCst);
        let stopped = db.reindex_collection_with_migrations("Users").await;
        assert!(stopped.is_err(), "the lens failure stops the rebuild");
        assert_eq!(
            db.owed_reindexes().await.unwrap(),
            vec![collection_id],
            "a rebuild that stopped part way stays owed"
        );
        assert!(
            db.query_schema("Users").unwrap().indexes.is_empty(),
            "its indexes answer no query until the last page commits"
        );

        lens.fail_call.store(0, Ordering::SeqCst);
        let resumed_from = lens.transform_calls.load(Ordering::SeqCst);
        assert_eq!(
            db.settle_owed_reindexes().await.unwrap(),
            ReindexReport {
                settled: 1,
                failed: 0
            }
        );
        let migrated_on_resume = lens.transform_calls.load(Ordering::SeqCst) - resumed_from;
        assert!(
            migrated_on_resume > 0 && migrated_on_resume < DOCS,
            "the settle resumes after the last page that committed, \
         migrating {migrated_on_resume} of {DOCS} documents"
        );
        assert!(db.owed_reindexes().await.unwrap().is_empty());
        assert_eq!(
            verified_index_count(&db).await,
            DOCS,
            "every document is indexed at its migrated value"
        );
        assert_eq!(db.query_schema("Users").unwrap().indexes.len(), 1);
    })
    .unwrap()
}
