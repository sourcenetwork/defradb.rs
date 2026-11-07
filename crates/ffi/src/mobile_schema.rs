//! Which collections a mobile schema bootstrap creates and which it skips.

use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;

/// The collections to create, and the names skipped because they already exist.
pub struct Bootstrap {
    pub create: Vec<CollectionVersion>,
    pub skipped: Vec<String>,
}

/// Splits `parsed` against the stored collections. A chunk collection goes with
/// its parent, so a parent is never created or skipped without its chunks.
pub fn plan_bootstrap(
    parsed: Vec<CollectionVersion>,
    stored: &[CollectionVersion],
) -> Result<Bootstrap, String> {
    let mut active = RapidHashMap::with_capacity(stored.len());
    for collection in stored.iter().filter(|c| c.is_active) {
        active.insert(collection.name.as_str(), collection);
    }
    let parents: RapidHashMap<&str, &CollectionVersion> =
        parsed.iter().map(|c| (c.name.as_str(), c)).collect();
    for (name, collection) in &parents {
        let Some(existing) = active.get(name) else {
            continue;
        };
        if existing.chunked_fields != collection.chunked_fields {
            return Err(format!(
                "the @chunk fields of {name} differ from the stored collection, and they cannot be added, removed or changed after it is created. Collection: {name}"
            ));
        }
    }
    let (mut create, mut skipped) = (Vec::new(), Vec::new());
    for collection in parsed {
        let group = collection
            .chunk_of
            .as_ref()
            .map_or(collection.name.as_str(), |source| {
                source.collection.as_str()
            });
        match active.contains_key(group) {
            true => skipped.push(collection.name.clone()),
            false => create.push(collection),
        }
    }
    Ok(Bootstrap { create, skipped })
}

#[cfg(test)]
#[path = "mobile_schema_tests.rs"]
mod tests;
