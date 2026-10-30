//! Shared collection loading utilities.
//!
//! This module provides common functions for loading collections from the SystemStore,
//! used by both DbDocFetcher and DbDocMutator.

use datastore::NamespaceView;
use schema::CollectionVersion;
use storage::corekv::{Key, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey};
use tracing::{error, warn};

use crate::collection::{populate_collection_root_id, Collection};
use crate::index::IndexManager;
use crate::txn::DbTxn;

/// Compare schema fields without cloning both definitions just to ignore
/// indexes. Indexes may change while preserving a collection version ID, but
/// all other persisted definition data must match the prepared schema. Keep
/// this list aligned with `CollectionVersion` when adding schema fields.
pub(crate) fn same_schema_except_indexes(
    left: &CollectionVersion,
    right: &CollectionVersion,
) -> bool {
    let CollectionVersion {
        name: left_name,
        version_id: left_version_id,
        collection_id: left_collection_id,
        collection_set: left_collection_set,
        query: left_query,
        previous_version: left_previous_version,
        fields: left_fields,
        indexes: _,
        encrypted_indexes: left_encrypted_indexes,
        fulltext_indexes: left_fulltext_indexes,
        policy: left_policy,
        policy_cid: left_policy_cid,
        governance_root: left_governance_root,
        is_active: left_is_active,
        is_materialized: left_is_materialized,
        downsample_interval: left_downsample_interval,
        downsample_time_field: left_downsample_time_field,
        downsample_retention: left_downsample_retention,
        downsample_source: left_downsample_source,
        is_branchable: left_is_branchable,
        is_embedded_only: left_is_embedded_only,
        is_placeholder: left_is_placeholder,
        vector_embeddings: left_vector_embeddings,
        root_id: left_root_id,
    } = left;
    let CollectionVersion {
        name: right_name,
        version_id: right_version_id,
        collection_id: right_collection_id,
        collection_set: right_collection_set,
        query: right_query,
        previous_version: right_previous_version,
        fields: right_fields,
        indexes: _,
        encrypted_indexes: right_encrypted_indexes,
        fulltext_indexes: right_fulltext_indexes,
        policy: right_policy,
        policy_cid: right_policy_cid,
        governance_root: right_governance_root,
        is_active: right_is_active,
        is_materialized: right_is_materialized,
        downsample_interval: right_downsample_interval,
        downsample_time_field: right_downsample_time_field,
        downsample_retention: right_downsample_retention,
        downsample_source: right_downsample_source,
        is_branchable: right_is_branchable,
        is_embedded_only: right_is_embedded_only,
        is_placeholder: right_is_placeholder,
        vector_embeddings: right_vector_embeddings,
        root_id: right_root_id,
    } = right;

    left_name == right_name
        && left_version_id == right_version_id
        && left_collection_id == right_collection_id
        && left_collection_set == right_collection_set
        && left_query == right_query
        && left_previous_version == right_previous_version
        && left_fields == right_fields
        && left_encrypted_indexes == right_encrypted_indexes
        && left_fulltext_indexes == right_fulltext_indexes
        && left_policy == right_policy
        && left_policy_cid == right_policy_cid
        && left_governance_root == right_governance_root
        && left_is_active == right_is_active
        && left_is_materialized == right_is_materialized
        && left_downsample_interval == right_downsample_interval
        && left_downsample_time_field == right_downsample_time_field
        && left_downsample_retention == right_downsample_retention
        && left_downsample_source == right_downsample_source
        && left_is_branchable == right_is_branchable
        && left_is_embedded_only == right_is_embedded_only
        && left_is_placeholder == right_is_placeholder
        && left_vector_embeddings == right_vector_embeddings
        && left_root_id == right_root_id
}

/// Compare persisted definitions while treating legacy `kind: None` index
/// declarations as their canonical ordered-index equivalent.
pub(crate) fn same_schema(left: &CollectionVersion, right: &CollectionVersion) -> bool {
    same_schema_except_indexes(left, right)
        && left.indexes.len() == right.indexes.len()
        && left
            .indexes
            .iter()
            .zip(&right.indexes)
            .all(|(left, right)| left.clone().normalized() == right.clone().normalized())
}

/// Load a collection for an engine-owned write attempt from that attempt's
/// snapshot. Index definitions are allowed to have changed since preparation,
/// because index creation currently preserves the collection version ID; the
/// collection fields and other schema content must still match. The active
/// mapping, definition, and action statuses are all read through this snapshot:
/// the reads of the name and the definition are what a definition change
/// committed by another transaction conflicts with.
pub(crate) async fn load_collection_for_write_attempt(
    systemstore: &NamespaceView,
    name: &str,
    expected: &Collection,
) -> crate::error::Result<Collection> {
    let collection = load_collection_from_systemstore_impl(systemstore, name, Some(expected))
        .await
        .map_err(|error| crate::error::Error::Other(error.to_string()))?
        .ok_or_else(|| crate::error::Error::CollectionNotFound(name.to_string()))?;
    if collection.version_id() != expected.version_id() {
        return Err(crate::error::Error::CollectionNotFound(name.to_string()));
    }
    if expected.schema() != collection.schema()
        && !same_schema_except_indexes(expected.schema(), collection.schema())
    {
        return Err(crate::error::Error::Other(format!(
            "collection schema changed during write preparation for '{}'",
            collection.name()
        )));
    }
    Ok(collection)
}

/// Load a collection from the systemstore by name.
///
/// This is a standalone async function that doesn't hold any locks,
/// allowing it to be called outside the mutex lock scope.
///
/// Storage layout uses two-step lookup:
/// - `/collection/name/{name}` contains the version_id string
/// - `/collection/id/{version_id}` contains the full JSON schema
///
/// Returns `Ok(Some(collection))` if found, `Ok(None)` if not found,
/// or an error if storage/deserialization fails.
pub(crate) async fn load_collection_from_systemstore(
    systemstore: &NamespaceView,
    name: &str,
) -> query::error::Result<Option<Collection>> {
    load_collection_from_systemstore_impl(systemstore, name, None).await
}

async fn load_collection_from_systemstore_impl(
    systemstore: &NamespaceView,
    name: &str,
    expected: Option<&Collection>,
) -> query::error::Result<Option<Collection>> {
    // Step 1: Get version_id from /collection/name/{name}
    let name_key = CollectionNameKey::new(name);

    let version_id = match systemstore.get(&name_key.bytes()).await.map_err(|e| {
        error!(
            error = ?e,
            collection_name = %name,
            "Storage error while loading collection name mapping"
        );
        query::error::QueryError::execution(format!("storage error: {}", e))
    })? {
        Some(data) => String::from_utf8(data.into()).map_err(|e| {
            error!(
                error = ?e,
                collection_name = %name,
                "Invalid UTF-8 in version_id for collection"
            );
            query::error::QueryError::execution(format!(
                "invalid version_id encoding for collection '{}': {}",
                name, e
            ))
        })?,
        None => return Ok(None),
    };

    // Step 2: Get full schema from /collection/id/{version_id}
    let collection_key = CollectionKey::new(&version_id);

    match systemstore
        .get(&collection_key.bytes())
        .await
        .map_err(|e| {
            error!(
                error = ?e,
                collection_name = %name,
                version_id = %version_id,
                "Storage error while loading collection definition"
            );
            query::error::QueryError::execution(format!("storage error: {}", e))
        })? {
        Some(data) => {
            if let Some(expected) = expected {
                if version_id == expected.version_id()
                    && expected.definition_wire() == Some(data.as_ref())
                {
                    let root_id = crate::collection::load_persisted_collection_short_id(
                        systemstore,
                        &expected.schema().collection_id,
                    )
                    .await
                    .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
                    if exact_schema_wire_matches(expected, &version_id, &data, root_id) {
                        let statuses = crate::collection::IndexStatuses::load_for(
                            systemstore,
                            expected.schema(),
                        )
                        .await
                        .map_err(|e| {
                            query::error::QueryError::execution(format!(
                                "failed to load index actions for collection '{}': {}",
                                name, e
                            ))
                        })?;
                        return Ok(Some(Collection::with_index_actions_reusing_schema(
                            expected, &statuses,
                        )));
                    }
                }
            }

            let mut schema: CollectionVersion = serde_json::from_slice(&data).map_err(|e| {
                error!(
                    error = ?e,
                    collection_name = %name,
                    version_id = %version_id,
                    "Failed to deserialize schema for collection"
                );
                query::error::QueryError::execution(format!(
                    "failed to deserialize schema for collection '{}': {}",
                    name, e
                ))
            })?;

            populate_collection_root_id(systemstore, &mut schema)
                .await
                .map_err(|e| {
                    query::error::QueryError::execution(format!(
                        "failed to load persisted root_id for collection '{}': {}",
                        name, e
                    ))
                })?;

            let statuses = crate::collection::IndexStatuses::load_for(systemstore, &schema)
                .await
                .map_err(|e| {
                    query::error::QueryError::execution(format!(
                        "failed to load index actions for collection '{}': {}",
                        name, e
                    ))
                })?;

            Ok(Some(Collection::with_index_actions(
                schema,
                &statuses,
                systemstore.crdts().clone(),
            )))
        }
        None => {
            // version_id found but schema missing - inconsistent state
            warn!(
                collection_name = %name,
                version_id = %version_id,
                "Collection name mapping exists but schema definition not found"
            );
            Ok(None)
        }
    }
}

pub(crate) fn exact_schema_wire_matches(
    expected: &Collection,
    version_id: &str,
    persisted_wire: &[u8],
    persisted_root_id: Option<u32>,
) -> bool {
    expected.schema().root_id > 0
        && version_id == expected.version_id()
        && expected.definition_wire() == Some(persisted_wire)
        && persisted_root_id == Some(expected.schema().root_id)
}

/// Get a collection by name with lazy loading from the SystemStore.
///
/// This function checks the transaction's cache first. On cache miss, it loads
/// the collection from the SystemStore and adds it to the cache.
///
/// Returns the collection, datastore, and systemstore (for DocID <-> short-ID
/// resolution) for document operations.
pub async fn get_collection_with_lazy_load<S: Store + 'static>(
    txn: &DbTxn<S>,
    collection_name: &str,
) -> query::error::Result<(Collection, NamespaceView, NamespaceView)> {
    let systemstore = txn.systemstore().map_err(|e| {
        query::error::QueryError::execution(format!(
            "failed to get systemstore for collection '{}': {}",
            collection_name, e
        ))
    })?;
    let datastore = txn.datastore().map_err(|e| {
        query::error::QueryError::execution(format!(
            "failed to get datastore for collection '{}': {}",
            collection_name, e
        ))
    })?;
    if let Some(collection) = txn.cached_collection(collection_name) {
        return Ok((collection, datastore, systemstore));
    }

    // Cache miss: load from SystemStore. A definition byte-equal to the
    // database's committed handle reuses that handle's decoded schema and
    // part-ID map instead of parsing them again.
    let shared = txn.shared_collection(collection_name);
    let collection =
        load_collection_from_systemstore_impl(&systemstore, collection_name, shared.as_ref())
            .await?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
    txn.cache_collection(collection.clone());
    Ok((collection, datastore, systemstore))
}

/// Get a collection by name with lazy loading and create an IndexManager.
///
/// This function is similar to `get_collection_with_lazy_load` but also creates
/// an IndexManager for the collection, which is needed for document mutations
/// that maintain index consistency (unique constraints, etc.).
///
/// Returns the collection, datastore, systemstore, and IndexManager for
/// document operations.
pub async fn get_collection_with_index_manager<S: Store + 'static>(
    txn: &DbTxn<S>,
    collection_name: &str,
) -> query::error::Result<(Collection, NamespaceView, NamespaceView, IndexManager)> {
    let (collection, datastore, systemstore) =
        get_collection_with_lazy_load(txn, collection_name).await?;

    // Create an IndexManager from the collection schema.
    let short_id = collection.resolved_root_id();
    let index_manager = IndexManager::from_indexes(
        short_id,
        collection.schema(),
        collection.write_indexes(),
        collection.crdts(),
    )
    .map_err(|e| {
        query::error::QueryError::execution(format!(
            "failed to create index manager for collection '{}': {}",
            collection_name, e
        ))
    })?
    .with_drains(txn.deferred_drains());

    Ok((collection, datastore, systemstore, index_manager))
}

#[cfg(test)]
mod tests {
    use super::{exact_schema_wire_matches, same_schema_except_indexes};
    use crate::collection::Collection;
    use schema::{CollectionVersion, IndexDescription};

    #[test]
    fn schema_comparison_allows_index_drift_but_rejects_other_changes() {
        let left = CollectionVersion::new("users", "version", "collection", vec![]);
        let mut right = left.clone();
        right.indexes.push(IndexDescription::new("by_name"));
        assert!(same_schema_except_indexes(&left, &right));

        right.is_branchable = true;
        assert!(!same_schema_except_indexes(&left, &right));
    }

    #[test]
    fn exact_definition_fast_path_requires_schema_and_runtime_root_match() {
        let mut schema = CollectionVersion::new("users", "version", "collection", vec![]);
        schema.root_id = 17;
        let expected = Collection::new(schema.clone(), crdt::Crdts::default());
        let wire = serde_json::to_vec(&schema).unwrap();

        assert!(exact_schema_wire_matches(
            &expected,
            "version",
            &wire,
            Some(17)
        ));
        assert!(!exact_schema_wire_matches(
            &expected,
            "version",
            &wire,
            Some(18)
        ));
        assert!(!exact_schema_wire_matches(
            &expected, "version", &wire, None
        ));

        schema.is_branchable = true;
        let stale_schema_wire = serde_json::to_vec(&schema).unwrap();
        assert!(!exact_schema_wire_matches(
            &expected,
            "version",
            &stale_schema_wire,
            Some(17)
        ));
        schema.is_branchable = false;
        schema.indexes.push(IndexDescription::new("by_name"));
        assert!(!exact_schema_wire_matches(
            &expected,
            "version",
            &serde_json::to_vec(&schema).unwrap(),
            Some(17)
        ));
        assert!(same_schema_except_indexes(expected.schema(), &schema));
    }
}
