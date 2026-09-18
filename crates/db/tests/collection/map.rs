//! The process-wide collection cache, keyed by collection ID with a name index
//! and a version index kept beside it.

use db::collection::Collection;
use db::{Cached, CollectionMap};
use schema::{CollectionVersion, FieldDescription, FieldKind};

fn version(name: &str, collection_id: &str, version_id: &str) -> Collection {
    Collection::new(CollectionVersion::new(
        name,
        version_id,
        collection_id,
        vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())],
    ))
}

fn placeholder(name: &str, collection_id: &str, version_id: &str) -> Collection {
    let mut schema = version(name, collection_id, version_id).schema().clone();
    schema.is_placeholder = true;
    Collection::new(schema)
}

/// Every lookup the cache offers reaches `collection_id`, and nothing else does.
fn assert_resolves(map: &CollectionMap, name: &str, collection_id: &str, version_id: &str) {
    let ids = |found: Option<&Collection>| found.map(|c| c.collection_id().to_string());
    assert_eq!(
        ids(map.get(name)),
        Some(collection_id.to_string()),
        "by name"
    );
    assert_eq!(
        ids(map.by_id(collection_id)),
        Some(collection_id.to_string()),
        "by id"
    );
    assert_eq!(
        ids(map.by_version(version_id)),
        Some(collection_id.to_string()),
        "by version"
    );
}

#[test]
fn a_put_collection_resolves_by_name_id_and_version() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-agent", "v1"));

    assert_resolves(&map, "Agent", "col-agent", "v1");
    assert_eq!(map.len(), 1);
    assert_eq!(map.names().collect::<Vec<_>>(), vec!["Agent"]);
}

#[test]
fn a_new_version_replaces_the_old_one_in_every_index() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-agent", "v1"));
    map.put(version("Agent", "col-agent", "v2"));

    assert_resolves(&map, "Agent", "col-agent", "v2");
    assert!(
        map.by_version("v1").is_none(),
        "a superseded version is no longer cached"
    );
    assert_eq!(map.len(), 1);
}

#[test]
fn offering_a_different_collection_for_a_held_name_leaves_it_alone() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-agent", "v1"));

    let outcome = map.offer(version("Agent", "col-peer", "p1"));

    assert_eq!(outcome, Cached::NameHeldByAnother);
    assert_resolves(&map, "Agent", "col-agent", "v1");
    assert!(map.by_id("col-peer").is_none());
    assert!(map.by_version("p1").is_none());
    assert_eq!(map.len(), 1);
}

#[test]
fn offering_takes_a_free_name_the_same_collection_or_a_placeholder() {
    let mut map = CollectionMap::default();

    assert_eq!(
        map.offer(version("Agent", "col-agent", "v1")),
        Cached::Taken
    );
    assert_resolves(&map, "Agent", "col-agent", "v1");

    assert_eq!(
        map.offer(version("Agent", "col-agent", "v2")),
        Cached::Taken
    );
    assert_resolves(&map, "Agent", "col-agent", "v2");

    map.put(placeholder("Ledger", "col-stand-in", "s1"));
    assert_eq!(
        map.offer(version("Ledger", "col-ledger", "l1")),
        Cached::Taken
    );
    assert_resolves(&map, "Ledger", "col-ledger", "l1");
    assert!(
        map.by_id("col-stand-in").is_none(),
        "the placeholder is gone, not kept nameless"
    );
}

/// `put` is for a version this node committed as current, so it takes the name
/// outright — the displaced collection leaves every index with it rather than
/// lingering, unreachable by name, in the others.
#[test]
fn putting_a_different_collection_takes_the_name_and_evicts_the_holder() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-peer", "p1"));

    map.put(version("Agent", "col-agent", "v1"));

    assert_resolves(&map, "Agent", "col-agent", "v1");
    assert!(map.by_id("col-peer").is_none());
    assert!(map.by_version("p1").is_none());
    assert_eq!(map.len(), 1);
}

#[test]
fn removing_a_name_clears_every_index() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-agent", "v1"));

    let removed = map.remove("Agent").expect("present");

    assert_eq!(removed.collection_id(), "col-agent");
    assert!(map.get("Agent").is_none());
    assert!(map.by_id("col-agent").is_none());
    assert!(map.by_version("v1").is_none());
    assert!(map.is_empty());
}

/// Validation refuses to rename a collection unless it is a placeholder, so a
/// placeholder being defined is the rename that reaches the cache. The entry
/// moves to its new name; the old name no longer reaches anything.
#[test]
fn a_collection_under_a_new_name_moves_rather_than_answering_to_both() {
    let mut map = CollectionMap::default();
    map.put(placeholder("Draft", "col-agent", "v1"));

    map.put(version("Agent", "col-agent", "v2"));

    assert_resolves(&map, "Agent", "col-agent", "v2");
    assert!(map.get("Draft").is_none());
    assert_eq!(map.len(), 1);
    assert_eq!(map.names().collect::<Vec<_>>(), vec!["Agent"]);
}

#[test]
fn iteration_and_snapshots_see_each_collection_once_under_its_name() {
    let mut map = CollectionMap::default();
    map.put(version("Agent", "col-agent", "v1"));
    map.put(version("Ledger", "col-ledger", "l1"));
    let _ = map.offer(version("Agent", "col-peer", "p1"));

    let mut named: Vec<_> = map
        .iter()
        .map(|(name, c)| (name.clone(), c.collection_id().to_string()))
        .collect();
    named.sort();
    assert_eq!(
        named,
        vec![
            ("Agent".to_string(), "col-agent".to_string()),
            ("Ledger".to_string(), "col-ledger".to_string()),
        ]
    );
    assert_eq!(map.values().count(), 2);
    assert_eq!(map.by_name().len(), 2);
}
