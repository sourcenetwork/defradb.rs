//! A merged document stored at a version other than the active one.
//!
//! Queries read a document through the lenses between its stored version and
//! the active one, but a merge indexes the values it stores. When those
//! versions differ across a lens, the entries may not describe what queries
//! read, so the merge owes the collection a reindex in its own transaction:
//! until the rebuild settles, no index answers queries and they scan; the
//! rebuild migrates the document to the active version and indexes it again.
//! The merge does not run the rebuild: like any owed reindex it settles after
//! the next schema change and at open.

use std::borrow::Cow;

use serde::de::IgnoredAny;
use serde::Deserialize;
use storage::corekv::{IterOptions, Key};
use storage::keys::systemstore::{CollectionKey, CollectionVersionKey};

use super::*;
use crate::definition::lens::build_collection_history;
use crate::definition::migration::owe_reindex;

/// The part of a stored definition that says whether it has indexes.
#[derive(Deserialize)]
struct StoredIndexes {
    #[serde(rename = "Indexes", default)]
    indexes: Option<Vec<IgnoredAny>>,
}

/// The parts of a stored definition its place in the version history needs.
#[derive(Deserialize)]
struct StoredHistoryLink {
    #[serde(rename = "VersionID", default)]
    version_id: String,
    #[serde(rename = "CollectionID", default)]
    collection_id: String,
    #[serde(rename = "PreviousVersion", default)]
    previous_version: Option<schema::CollectionSource>,
}

fn decode<'de, T>(version_id: &str, json: &'de [u8]) -> std::result::Result<T, MergeError>
where
    T: Deserialize<'de>,
{
    serde_json::from_slice(json).map_err(|error| {
        MergeError::Database(crate::error::Error::collection_schema_json(
            format!("failed to deserialize collection version '{version_id}'"),
            error,
        ))
    })
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Owes `collection`'s collection a reindex, in the merge's transaction,
    /// when the active version `active_version` has indexes and reads the
    /// document, stored at another version of it, through a lens. The active
    /// version is the one the merge's validated name read returned, and
    /// `stored_definition` is the definition of `collection`'s version that
    /// the same validation read. Every definition the decision needs is read
    /// in the merge's transaction, validated, and decoded only for the parts
    /// it decides on, so a schema change that commits first aborts the merge
    /// and the re-run decides again. It owes a full rebuild every time, so a
    /// rebuild already running when the document arrived does not clear the
    /// record without indexing it, and returns whether it owed one.
    pub(super) async fn owe_reindex_off_active_version(
        &self,
        systemstore: &NamespaceView,
        collection: &Collection,
        active_version: &str,
        stored_definition: &[u8],
    ) -> std::result::Result<bool, MergeError> {
        let active_definition: Cow<'_, [u8]> = if collection.version_id() == active_version {
            Cow::Borrowed(stored_definition)
        } else {
            match systemstore
                .get(&CollectionKey::new(active_version).bytes())
                .await
                .map_err(|e| MergeError::Database(crate::error::Error::Storage(e)))?
            {
                Some(json) => Cow::Owned(json.to_vec()),
                None => return Ok(false),
            }
        };
        // `collection`'s decoded definition, when the stored one is
        // byte-equal to the wire it was decoded from.
        let known_active = (collection.version_id() == active_version
            && collection.definition_wire() == Some(&active_definition[..]))
        .then(|| collection.schema());
        let indexed = match known_active {
            Some(active) => !active.indexes.is_empty(),
            None => decode::<StoredIndexes>(active_version, &active_definition)?
                .indexes
                .is_some_and(|indexes| !indexes.is_empty()),
        };
        if !indexed {
            return Ok(false);
        }
        let collection_id = collection.collection_id();
        // The active version's own link is in every history built toward it,
        // so a lens into the active version answers without the others.
        let has_transform = |link: Option<&schema::CollectionSource>| {
            link.is_some_and(|link| link.transform.is_some())
        };
        let lens_into_active = match known_active {
            Some(active) => has_transform(active.previous_version.as_ref()),
            None => has_transform(
                decode::<StoredHistoryLink>(active_version, &active_definition)?
                    .previous_version
                    .as_ref(),
            ),
        };
        let across_lens = if lens_into_active {
            true
        } else {
            let versions = self
                .history_links(
                    systemstore,
                    collection_id,
                    active_version,
                    &active_definition,
                    known_active,
                )
                .await?;
            build_collection_history(&versions, active_version)
                .is_some_and(|history| history.values().any(|link| link.transform.is_some()))
        };
        if !across_lens {
            return Ok(false);
        }
        owe_reindex(systemstore, &self.db.collections, collection_id, None)
            .await
            .map_err(MergeError::Database)?;
        Ok(true)
    }

    /// Every version of `collection_id`, with only what its history link
    /// needs: the version list is a validated scan, so a version added
    /// concurrently aborts the merge, and each definition a validated read,
    /// except the active one, already read as `active_definition` and,
    /// when `known_active` is its decoded form, not decoded again.
    async fn history_links(
        &self,
        systemstore: &NamespaceView,
        collection_id: &str,
        active_version: &str,
        active_definition: &[u8],
        known_active: Option<&schema::CollectionVersion>,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, MergeError> {
        let storage_error = |e| MergeError::Database(crate::error::Error::Storage(e));
        let mut version_ids = vec![collection_id.to_string()];
        let prefix = CollectionVersionKey::collection_prefix(collection_id);
        let mut iter = systemstore
            .iterator(
                IterOptions::new()
                    .with_prefix(prefix)
                    .with_keys_only(true)
                    .with_validation(true),
            )
            .await
            .map_err(storage_error)?;
        loop {
            match iter.next().await {
                Ok(Some(kv)) => {
                    if let Some(version_id) = std::str::from_utf8(&kv.key)
                        .ok()
                        .and_then(|key| key.rsplit('/').next())
                        .filter(|id| !id.is_empty() && *id != collection_id)
                    {
                        version_ids.push(version_id.to_string());
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = iter.close().await;
                    return Err(storage_error(e));
                }
            }
        }
        iter.close().await.map_err(storage_error)?;

        let mut versions = Vec::with_capacity(version_ids.len());
        for version_id in &version_ids {
            if let Some(active) = known_active.filter(|_| version_id == active_version) {
                let mut version = schema::CollectionVersion::new(
                    "",
                    active.version_id.clone(),
                    active.collection_id.clone(),
                    Vec::new(),
                );
                version.previous_version = active.previous_version.clone();
                versions.push(version);
                continue;
            }
            let read;
            let json = if version_id == active_version {
                active_definition
            } else {
                read = systemstore
                    .get(&CollectionKey::new(version_id).bytes())
                    .await
                    .map_err(storage_error)?;
                match read.as_deref() {
                    Some(json) => json,
                    None => continue,
                }
            };
            let link = decode::<StoredHistoryLink>(version_id, json)?;
            let mut version =
                schema::CollectionVersion::new("", link.version_id, link.collection_id, Vec::new());
            version.previous_version = link.previous_version;
            versions.push(version);
        }
        Ok(versions)
    }
}
