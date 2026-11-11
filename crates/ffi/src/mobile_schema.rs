//! Which collections a mobile schema bootstrap creates and which it skips.

use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;

/// The collections to create, and the names skipped because they already exist.
pub struct Bootstrap {
    pub create: Vec<CollectionVersion>,
    pub skipped: Vec<String>,
}

/// Splits `parsed` against the stored collections. A name exists if any version
/// of it is stored, active or not, so a collection synced from a peer is skipped.
/// A chunk collection goes with its parent, so a parent is never created or
/// skipped without its chunks.
pub fn plan_bootstrap(
    parsed: Vec<CollectionVersion>,
    stored: &[CollectionVersion],
) -> Result<Bootstrap, String> {
    let existing = existing_by_name(stored);
    for collection in &parsed {
        if let Some(Some(stored)) = existing.get(collection.name.as_str()) {
            schema::ensure_chunk_settings_kept(stored, collection)
                .map_err(|error| error.to_string())?;
        }
    }
    let (mut create, mut skipped) = (Vec::new(), Vec::new());
    for collection in parsed {
        let group = collection
            .chunk_of
            .as_ref()
            .map_or(collection.name.as_str(), |parent| {
                parent.collection.as_str()
            });
        match existing.contains_key(group) {
            true => skipped.push(collection.name.clone()),
            false => create.push(collection),
        }
    }
    Ok(Bootstrap { create, skipped })
}

/// Every stored name, with the version its `@chunk` settings are checked
/// against. That is the first version, which activation holds every version to.
fn existing_by_name(
    stored: &[CollectionVersion],
) -> RapidHashMap<&str, Option<&CollectionVersion>> {
    let mut existing: RapidHashMap<&str, Option<&CollectionVersion>> =
        RapidHashMap::with_capacity(stored.len());
    for version in stored {
        let kept = existing.entry(version.name.as_str()).or_insert(None);
        if settings_rank(version) > kept.map_or(0, settings_rank) {
            *kept = Some(version);
        }
    }
    existing
}

/// A placeholder has no fields yet, so it says nothing about `@chunk`.
fn settings_rank(version: &CollectionVersion) -> u8 {
    match (version.is_placeholder, version.previous_version.is_none()) {
        (true, _) => 0,
        (false, false) => 1,
        (false, true) => 2,
    }
}

#[cfg(test)]
#[path = "mobile_schema_tests.rs"]
mod tests;
