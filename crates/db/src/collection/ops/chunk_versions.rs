use rapidhash::RapidHashSet;
use schema::CollectionVersion;

use crate::error::{Error, Result};

/// The chunk collection versions that go with `version_ids`. A chunk collection
/// can't outlive its parent, so deleting one whose parent stays is refused.
pub(super) fn chunk_versions_to_add(
    all_versions: &[CollectionVersion],
    version_ids: &[String],
) -> Result<Vec<String>> {
    let deleting: RapidHashSet<&str> = version_ids.iter().map(String::as_str).collect();
    let mut added: Vec<String> = Vec::new();

    for parent in all_versions {
        if !deleting.contains(parent.version_id.as_str()) || !parent.is_active {
            continue;
        }
        let whole_parent_goes = all_versions
            .iter()
            .filter(|other| other.collection_id == parent.collection_id)
            .all(|other| deleting.contains(other.version_id.as_str()));
        for chunked in &parent.chunked_fields {
            let chunks = chunked.collection(&parent.name);
            let Some(active) = all_versions
                .iter()
                .find(|other| other.is_active && other.name == chunks)
            else {
                continue;
            };
            for version in all_versions.iter().filter(|other| {
                other.collection_id == active.collection_id
                    && (other.version_id == active.version_id || whole_parent_goes)
            }) {
                if !deleting.contains(version.version_id.as_str())
                    && !added.contains(&version.version_id)
                {
                    added.push(version.version_id.clone());
                }
            }
        }
    }

    let going = |id: &str| deleting.contains(id) || added.iter().any(|added| added == id);
    for version in all_versions {
        let Some(source) = &version.chunk_of else {
            continue;
        };
        if !version.is_active || !going(&version.version_id) {
            continue;
        }
        let parent_stays = all_versions.iter().any(|other| {
            other.is_active && other.name == source.collection && !going(&other.version_id)
        });
        if parent_stays {
            return Err(Error::DeleteGeneratedCollection(version.name.clone()));
        }
    }
    Ok(added)
}
