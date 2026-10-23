//! The lensed fetcher's migration-context cache: one immutable snapshot per
//! committed migration-graph generation, read without a lock.

use std::sync::Arc;

use super::super::LensedAutoCommitFetcher;
use super::{MigrationCache, MigrationContext, Snapshot};
use crate::collection::Collection;
use crate::DB;
use storage::RegolithStore;

fn users(version_id: &str) -> Collection {
    Collection::new(schema::CollectionVersion::new(
        "Users",
        version_id,
        "users-collection",
        vec![schema::FieldDescription::new(
            "1",
            "name",
            schema::FieldKind::string(),
        )],
    ))
}

fn fetcher() -> (
    Arc<DB<RegolithStore>>,
    LensedAutoCommitFetcher<RegolithStore>,
) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let fetcher = LensedAutoCommitFetcher::new(Arc::clone(&db));
    (db, fetcher)
}

fn snapshot(cache: &MigrationCache) -> (u64, usize) {
    cache
        .snapshot
        .peek(|snapshot| (snapshot.generation, snapshot.contexts.len()))
}

fn context() -> Arc<MigrationContext> {
    Arc::new((false, None))
}

#[test]
fn a_hit_is_served_from_the_snapshot_of_its_generation() {
    workers::block_on(async {
        let (db, fetcher) = fetcher();
        let collection = users("v1");

        let (generation, has_migrations, history) =
            fetcher.load_migration_context(&collection).await.unwrap();
        assert!(!has_migrations);
        assert!(history.is_none());
        assert_eq!(snapshot(&fetcher.migration_cache), (generation, 1));
        let first = fetcher
            .migration_cache
            .get(generation, "users-collection:v1")
            .unwrap();

        fetcher.load_migration_context(&collection).await.unwrap();
        let second = fetcher
            .migration_cache
            .get(generation, "users-collection:v1")
            .unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "a hit reads the published context, it does not compute it again"
        );

        db.bump_migration_generation();
        let (next, _, _) = fetcher.load_migration_context(&collection).await.unwrap();
        assert_eq!(next, generation + 1);
        assert_eq!(
            snapshot(&fetcher.migration_cache),
            (next, 1),
            "a new generation starts from an empty snapshot"
        );
        assert!(fetcher
            .migration_cache
            .get(generation, "users-collection:v1")
            .is_none());
    })
    .unwrap()
}

#[test]
fn a_context_is_published_only_at_its_generation_or_after_an_older_one() {
    let cache = MigrationCache::default();
    assert!(cache.publish(1, "a", &context()));
    assert!(cache.publish(1, "b", &context()));
    assert_eq!(snapshot(&cache), (1, 2));

    assert!(cache.publish(2, "c", &context()));
    assert_eq!(
        snapshot(&cache),
        (2, 1),
        "the contexts of an older generation are dropped whole"
    );
    assert!(
        !cache.publish(1, "a", &context()),
        "a context computed under an older generation is never published"
    );
    assert_eq!(snapshot(&cache), (2, 1));
    assert!(cache.get(1, "c").is_none());
    assert!(cache.get(2, "c").is_some());
}

#[test]
fn a_published_snapshot_never_changes() {
    let one = Snapshot::default().with(1, "a", &context()).unwrap();
    let both = one.with(1, "b", &context()).unwrap();
    assert_eq!(one.contexts.len(), 1);
    assert_eq!(both.contexts.len(), 2);
    assert!(both.with(0, "c", &context()).is_none());
}

#[test]
fn concurrent_loads_publish_every_context_into_one_snapshot() {
    workers::block_on(async {
        let (_db, fetcher) = fetcher();
        let fetcher = Arc::new(fetcher);
        let mut tasks = Vec::new();
        for index in 0..32 {
            let fetcher = Arc::clone(&fetcher);
            tasks.push(
                workers::spawn(
                    Some(crate::common::worker_io::pool()),
                    "test",
                    move || async move {
                        let collection = users(&format!("v{}", index % 8));
                        fetcher.load_migration_context(&collection).await.unwrap().0
                    },
                )
                .unwrap(),
            );
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), 0);
        }
        assert_eq!(snapshot(&fetcher.migration_cache), (0, 8));
    })
    .unwrap()
}
