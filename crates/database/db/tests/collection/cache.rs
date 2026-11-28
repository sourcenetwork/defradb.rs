use db::collection::cache::*;
use db::collection::Collection;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;

fn test_collection(name: &str) -> Collection {
    Collection::new(
        CollectionVersion::new(
            name,
            "v1",
            format!("col-{}", name.to_lowercase()),
            vec![FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            )],
        ),
        crdt::Crdts::default(),
    )
}

#[test]
fn test_cache_new_is_empty() {
    let cache = CollectionCache::new();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert!(!cache.is_fully_populated());
}

#[test]
fn test_cache_add_and_get() {
    let mut cache = CollectionCache::new();
    let col = test_collection("Users");

    cache.add(col);

    assert!(cache.contains("Users"));
    assert!(!cache.contains("Posts"));
    assert_eq!(cache.len(), 1);

    let retrieved = cache.get("Users");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "Users");
}

#[test]
fn test_cache_remove() {
    let mut cache = CollectionCache::new();
    cache.add(test_collection("Users"));
    cache.add(test_collection("Posts"));

    assert_eq!(cache.len(), 2);

    let removed = cache.remove("Users");
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().name(), "Users");
    assert_eq!(cache.len(), 1);
    assert!(!cache.contains("Users"));
    assert!(cache.contains("Posts"));
}

#[test]
fn test_cache_remove_resets_fully_populated() {
    let mut cache = CollectionCache::new();
    cache.populate(vec![test_collection("Users"), test_collection("Posts")]);

    assert!(cache.is_fully_populated());
    assert_eq!(cache.len(), 2);

    cache.remove("Users");

    assert!(
        !cache.is_fully_populated(),
        "Cache should no longer be fully populated after remove"
    );
}

#[test]
fn test_cache_names() {
    let mut cache = CollectionCache::new();
    cache.add(test_collection("Users"));
    cache.add(test_collection("Posts"));

    let mut names = cache.names();
    names.sort();
    assert_eq!(names, vec!["Posts", "Users"]);
}

#[test]
fn test_cache_populate() {
    let mut cache = CollectionCache::new();
    assert!(!cache.is_fully_populated());

    cache.populate(vec![test_collection("Users"), test_collection("Posts")]);

    assert!(cache.is_fully_populated());
    assert_eq!(cache.len(), 2);
    assert!(cache.contains("Users"));
    assert!(cache.contains("Posts"));
}

#[test]
fn a_clone_is_a_snapshot_the_original_no_longer_changes() {
    let mut cache = CollectionCache::new();
    cache.populate(vec![test_collection("Users"), test_collection("Posts")]);
    let snapshot = cache.clone();

    cache.add(test_collection("Orders"));
    cache.remove("Users");

    let mut names = snapshot.names();
    names.sort();
    assert_eq!(names, ["Posts", "Users"]);
    assert!(snapshot.is_fully_populated());
    let mut names = cache.names();
    names.sort();
    assert_eq!(names, ["Orders", "Posts"]);
    assert!(!cache.is_fully_populated());
}

/// More collections than the recent layer holds, so some are folded.
const SPANNING: usize = 40;

fn spanning_cache() -> CollectionCache {
    let mut cache = CollectionCache::new();
    for index in 0..SPANNING {
        cache.add(test_collection(&format!("C{index}")));
    }
    cache
}

#[test]
fn every_load_stays_visible_across_folds() {
    let cache = spanning_cache();

    assert_eq!(cache.len(), SPANNING);
    for index in 0..SPANNING {
        let name = format!("C{index}");
        assert_eq!(cache.get(&name).map(|c| c.name().to_owned()), Some(name));
    }
    let mut names = cache.names();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), SPANNING, "each name listed once");
}

#[test]
fn a_reload_of_a_folded_name_answers_the_new_collection() {
    let mut cache = spanning_cache();
    let reloaded = Collection::new(
        CollectionVersion::new("C0", "v2", "col-c0", vec![]),
        crdt::Crdts::default(),
    );

    cache.add(reloaded);

    assert_eq!(
        cache.get("C0").map(|c| c.version_id().to_owned()),
        Some("v2".into())
    );
    assert_eq!(cache.len(), SPANNING, "a reload adds no entry");
    assert_eq!(cache.names().iter().filter(|name| *name == "C0").count(), 1);
}

#[test]
fn a_removal_clears_a_name_from_both_layers() {
    let mut cache = spanning_cache();
    cache.add(test_collection("C0"));

    assert!(cache.remove("C0").is_some());

    assert!(!cache.contains("C0"));
    assert!(cache.get("C0").is_none());
    assert_eq!(cache.len(), SPANNING - 1);
    assert!(cache.remove("C0").is_none());
}

#[test]
fn a_snapshot_taken_before_a_fold_keeps_its_contents() {
    let mut cache = CollectionCache::new();
    cache.add(test_collection("Users"));
    let snapshot = cache.clone();

    for index in 0..SPANNING {
        cache.add(test_collection(&format!("C{index}")));
    }
    cache.remove("Users");

    assert_eq!(snapshot.names(), ["Users"]);
    assert_eq!(cache.len(), SPANNING);
}

#[test]
fn active_collection_versions_exclude_inactive_cache_entries() {
    workers::block_on(async {
        let db = db::DB::open(storage::backends::RegolithStore::in_memory().unwrap())
            .await
            .expect("open");

        let active = test_collection("Users").schema().clone();
        let mut inactive = test_collection("Orders").schema().clone();
        inactive.is_active = false;
        assert_eq!(
            db.add_collection_to_cache(active)
                .await
                .expect("cache active"),
            db::Cached::Taken
        );
        assert_eq!(
            db.add_collection_to_cache(inactive)
                .await
                .expect("cache inactive"),
            db::Cached::Taken
        );

        let versions = db
            .get_active_collection_versions()
            .expect("read active versions");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].name, "Users");
    })
    .unwrap()
}
