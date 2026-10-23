/// Collection struct for DefraDB matching Go's internal/db/collection.go.
///
/// A Collection represents a set of documents that share the same schema.
/// It provides CRUD operations for documents.
///
/// # Transaction Semantics
///
/// Methods that perform both document storage and index updates (e.g., `create_with_indexes`,
/// `update_with_indexes`, `delete_with_indexes`) are NOT atomic within a single operation.
/// If the document write succeeds but the index update fails, the caller MUST discard the
/// transaction (do not commit) to maintain consistency. The underlying transaction will
/// roll back both operations when discarded.
use crate::error::{Error, Result};
use crate::index::IndexManager;
use datastore::NamespaceView;
use document::{DocID, Document, NormalValue};
use schema::{
    legacy_collection_short_id, CollectionVersion, FieldKind, IndexDescription, ScalarArrayKind,
    ScalarKind,
};
use std::sync::Arc;
use storage::corekv::{IterOptions, Key};
use storage::keys::doc_id_index::encode_doc_short_id;
use storage::keys::systemstore::{CollectionID, CollectionIDSequenceKey};

pub mod acp;
pub mod cache;
mod crud;
pub(crate) mod epoch;
mod index;
pub(crate) mod index_status;
pub mod loader;
pub mod map;
pub mod name;
pub(crate) mod ops;
mod series;
pub use map::{Cached, CollectionMap};
pub(crate) mod provider;
pub mod retriever;
pub mod selector;
pub mod snapshot;
pub mod stream;
mod truncator;
mod unchanged;
pub mod validation;

pub(crate) use index_status::IndexStatuses;
pub use truncator::DbCollectionTruncator;

/// Derive the legacy short ID from a collection_id string.
///
/// This is retained only for compatibility with older metadata and tests. Store-backed code
/// should resolve or require the persisted `root_id` instead.
#[deprecated(note = "use persisted collection root IDs instead")]
pub fn collection_short_id(collection_id: &str) -> u32 {
    legacy_collection_short_id(collection_id)
}

/// Load the persisted short ID for a collection if one exists.
pub async fn load_persisted_collection_short_id(
    systemstore: &NamespaceView,
    collection_id: &str,
) -> Result<Option<u32>> {
    let short_id_key = CollectionID::new(collection_id);
    let Some(short_id_bytes) = systemstore
        .get(&short_id_key.bytes())
        .await
        .map_err(Error::Storage)?
    else {
        return Ok(None);
    };

    let Ok(short_id_str) = String::from_utf8(short_id_bytes.to_vec()) else {
        return Ok(None);
    };

    Ok(short_id_str.parse::<u32>().ok())
}

/// Load the persisted root ID for a collection, returning an error if the mapping is missing.
pub async fn require_persisted_collection_short_id(
    systemstore: &NamespaceView,
    collection_id: &str,
) -> Result<u32> {
    load_persisted_collection_short_id(systemstore, collection_id)
        .await?
        .ok_or_else(|| {
            Error::Other(format!(
                "missing persisted collection root_id for collection_id '{}'",
                collection_id
            ))
        })
}

/// Load the persisted root ID for a collection, allocating one if it does not exist yet.
pub async fn ensure_persisted_collection_short_id(
    systemstore: &NamespaceView,
    collection_id: &str,
) -> Result<u32> {
    if let Some(short_id) = load_persisted_collection_short_id(systemstore, collection_id).await? {
        return Ok(short_id);
    }

    let seq_key = CollectionIDSequenceKey;
    let key_bytes = seq_key.bytes();
    let current: u32 = match systemstore.get(&key_bytes).await.map_err(Error::Storage)? {
        Some(bytes) if bytes.len() == 4 => {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => 0,
    };
    let next = current + 1;
    systemstore
        .set(&key_bytes, &next.to_be_bytes())
        .await
        .map_err(Error::Storage)?;

    let short_id_key = CollectionID::new(collection_id);
    systemstore
        .set(&short_id_key.bytes(), next.to_string().as_bytes())
        .await
        .map_err(Error::Storage)?;

    Ok(next)
}

/// Populate a deserialized collection schema with its persisted root ID.
pub async fn populate_collection_root_id(
    systemstore: &NamespaceView,
    schema: &mut CollectionVersion,
) -> Result<()> {
    if schema.root_id == 0 {
        schema.root_id =
            require_persisted_collection_short_id(systemstore, &schema.collection_id).await?;
    }
    Ok(())
}

/// Key prefix for document data in datastore.
pub(super) const DOC_KEY_PREFIX: &[u8] = b"/d/";

/// Key prefix for per-document schema versions in datastore.
pub(super) const VERSION_KEY_PREFIX: &[u8] = b"/v/";

/// Marker byte indicating a document is deleted (matches Go's DeletedObjectMarker).
pub(super) const DELETED_MARKER: u8 = 0x01;

/// A collection of documents with a shared schema.
#[derive(Debug, Clone)]
pub struct Collection {
    /// The collection schema definition.
    def: Arc<CollectionVersion>,
    /// A canonical serialized definition captured once when this immutable
    /// collection is built. It is only a fast-path hint: snapshot readers must
    /// still fetch the persisted value and require an exact byte match.
    definition_wire: Option<Arc<[u8]>>,
    write_indexes: Vec<IndexDescription>,
    /// The indexes that were built and that no action is changing.
    built_indexes: Vec<IndexDescription>,
    /// A reindex is owed, so no index answers queries ([`index_status`]).
    reindex_owed: bool,
    /// `def` with the queryable index set substituted, built once here
    /// because a `Collection` never changes after construction. Query-side
    /// callers take a pointer clone of this instead of deep-copying the
    /// definition on every lookup.
    query_schema: Arc<CollectionVersion>,
    /// This version's decoded part-ID map, shared with every handle built
    /// from the same definition (see `parts`).
    part_id_snapshot: Arc<parts::PartIdSnapshot>,
}

/// The indexes of `built_indexes` that answer queries.
fn queryable(built_indexes: &[IndexDescription], reindex_owed: bool) -> &[IndexDescription] {
    if reindex_owed {
        &[]
    } else {
        built_indexes
    }
}

/// `def` with the queryable index set substituted in.
fn query_schema_of(
    def: &Arc<CollectionVersion>,
    queryable_indexes: &[IndexDescription],
) -> Arc<CollectionVersion> {
    if def.indexes == queryable_indexes {
        return Arc::clone(def);
    }
    let mut schema = def.as_ref().clone();
    schema.indexes = queryable_indexes.to_vec();
    Arc::new(schema)
}

fn serialize_definition_hint(def: &CollectionVersion) -> Option<Arc<[u8]>> {
    serde_json::to_vec(def).ok().map(Arc::from)
}

impl Collection {
    pub(crate) async fn load_index_actions(
        def: CollectionVersion,
        systemstore: &datastore::NamespaceView,
    ) -> crate::error::Result<Self> {
        let statuses = IndexStatuses::load_for(systemstore, &def).await?;
        Ok(Self::with_index_actions(def, &statuses))
    }

    /// Create a new collection with the given schema definition.
    pub fn new(def: CollectionVersion) -> Self {
        let def = Arc::new(def);
        let definition_wire = serialize_definition_hint(&def);
        let indexes = def.indexes.clone();
        let query_schema = query_schema_of(&def, &indexes);
        Self {
            def,
            definition_wire,
            write_indexes: indexes.clone(),
            built_indexes: indexes,
            reindex_owed: false,
            query_schema,
            part_id_snapshot: Arc::default(),
        }
    }

    pub(crate) fn with_index_actions(def: CollectionVersion, statuses: &IndexStatuses) -> Self {
        let def = Arc::new(def);
        let definition_wire = serialize_definition_hint(&def);
        Self::with_index_actions_and_wire(def, definition_wire, Arc::default(), statuses)
    }

    /// `def` with `statuses`, keeping no serialized definition: for a handle
    /// that is only compared, never loaded through.
    pub(crate) fn with_index_actions_unwired(
        def: CollectionVersion,
        statuses: &IndexStatuses,
    ) -> Self {
        Self::with_index_actions_and_wire(Arc::new(def), None, Arc::default(), statuses)
    }

    /// `expected`'s definition with fresh index statuses. The definition is
    /// the same version, so its decoded part-ID map is shared as well.
    pub(crate) fn with_index_actions_reusing_schema(
        expected: &Self,
        statuses: &IndexStatuses,
    ) -> Self {
        Self::with_index_actions_and_wire(
            Arc::clone(&expected.def),
            expected.definition_wire.clone(),
            Arc::clone(&expected.part_id_snapshot),
            statuses,
        )
    }

    fn with_index_actions_and_wire(
        def: Arc<CollectionVersion>,
        definition_wire: Option<Arc<[u8]>>,
        part_id_snapshot: Arc<parts::PartIdSnapshot>,
        statuses: &IndexStatuses,
    ) -> Self {
        let write_indexes = def
            .indexes
            .iter()
            .filter(|index| statuses.takes_writes(index))
            .cloned()
            .collect();
        let built_indexes: Vec<IndexDescription> = def
            .indexes
            .iter()
            .filter(|index| statuses.is_built(index))
            .cloned()
            .collect();
        let reindex_owed = statuses.reindex_owed();
        let query_schema = query_schema_of(&def, queryable(&built_indexes, reindex_owed));
        Self {
            def,
            definition_wire,
            write_indexes,
            built_indexes,
            reindex_owed,
            query_schema,
            part_id_snapshot,
        }
    }

    /// This collection with a reindex owed or settled, as the cache publishes
    /// it when the owed record commits or is cleared.
    pub(crate) fn with_reindex_owed(&self, reindex_owed: bool) -> Self {
        Self {
            reindex_owed,
            query_schema: query_schema_of(&self.def, queryable(&self.built_indexes, reindex_owed)),
            ..self.clone()
        }
    }

    /// Whether `other` has the same indexes taking writes and built: what a
    /// writer checks its cached definition against. An owed reindex does not
    /// count, since it changes only which indexes answer queries.
    pub(crate) fn same_index_statuses(&self, other: &Self) -> bool {
        let normalize = |indexes: &[IndexDescription]| {
            indexes
                .iter()
                .cloned()
                .map(IndexDescription::normalized)
                .collect::<Vec<_>>()
        };
        normalize(&self.write_indexes) == normalize(&other.write_indexes)
            && normalize(&self.built_indexes) == normalize(&other.built_indexes)
    }

    /// Get the collection name.
    pub fn name(&self) -> &str {
        &self.def.name
    }

    /// Get the collection ID.
    pub fn collection_id(&self) -> &str {
        &self.def.collection_id
    }

    /// Get the collection version ID (content hash of this schema version).
    ///
    /// Go uses `VersionID()` for the `collectionVersionID` field in CRDT delta blocks.
    pub fn version_id(&self) -> &str {
        &self.def.version_id
    }

    /// Get the collection schema.
    pub fn schema(&self) -> &CollectionVersion {
        &self.def
    }

    pub(crate) fn definition_wire(&self) -> Option<&[u8]> {
        self.definition_wire.as_deref()
    }

    /// Get the storage prefix ID used for indexes, heads, and view cache keys.
    pub fn resolved_root_id(&self) -> u32 {
        self.def.resolved_root_id()
    }

    /// Get all indexes defined on this collection.
    pub fn get_indexes(&self) -> &[IndexDescription] {
        &self.def.indexes
    }

    pub(crate) fn write_indexes(&self) -> &[IndexDescription] {
        &self.write_indexes
    }

    /// The indexes that answer queries: the built ones, none while a reindex is owed.
    pub(crate) fn queryable_indexes(&self) -> &[IndexDescription] {
        queryable(&self.built_indexes, self.reindex_owed)
    }

    pub(crate) fn schema_for_queries(&self) -> Arc<CollectionVersion> {
        Arc::clone(&self.query_schema)
    }

    /// The collection schema, shared: what a job computing on another worker
    /// is handed.
    #[cfg(feature = "native")]
    pub(crate) fn shared_schema(&self) -> Arc<CollectionVersion> {
        Arc::clone(&self.def)
    }

    /// Check if an index exists on this collection.
    pub fn has_index(&self, name: &str) -> bool {
        self.def.indexes.iter().any(|idx| idx.name == name)
    }

    /// Get an index by name.
    pub fn get_index(&self, name: &str) -> Option<&IndexDescription> {
        self.def.indexes.iter().find(|idx| idx.name == name)
    }

    /// Record the definition and active-name mapping in the writer's read set.
    /// Historical merge inputs may name an older version, but still validate
    /// both that definition and the current mapping against concurrent DDL.
    pub(crate) async fn validate_write_definition(
        &self,
        datastore: &NamespaceView,
        require_current: bool,
    ) -> Result<()> {
        use storage::corekv::Key;
        use storage::keys::systemstore::{CollectionKey, CollectionNameKey};
        let system = datastore.sibling(storage::namespace::Namespace::Systemstore);
        let active = system
            .get(&CollectionNameKey::new(self.name()).bytes())
            .await
            .map_err(Error::Storage)?;
        let Some(active) = active else {
            return Err(Error::CollectionNotFound(self.name().to_string()));
        };
        if require_current && active.as_ref() != self.version_id().as_bytes() {
            return Err(Error::Storage(storage::Error::TxnConflict));
        }
        if system
            .get(&CollectionKey::new(self.version_id()).bytes())
            .await
            .map_err(Error::Storage)?
            .is_none()
        {
            return Err(Error::CollectionNotFound(self.name().to_string()));
        }
        Ok(())
    }

    /// Generate the storage key for a document blob.
    ///
    /// Keyed by the node-local doc short ID: iteration order over a
    /// collection is allocation (insertion) order, matching Go v1.0.0's
    /// short-ID-keyed datastore (#4838). Delegates to the shared key helper
    /// so the write, merge, and index layers agree on the layout (#1111).
    pub fn doc_key(&self, doc_short_id: u64) -> Vec<u8> {
        storage::keys::doc_key(&self.def.collection_id, doc_short_id)
    }

    /// Generate the storage key for a document's deletion marker.
    pub(crate) fn deleted_key(&self, doc_short_id: u64) -> Vec<u8> {
        storage::keys::deleted_doc_key(&self.def.collection_id, doc_short_id)
    }

    /// Generate the storage key for a document's schema version.
    ///
    /// The version is stored separately from the document data to enable
    /// efficient version checks without deserializing the full document.
    ///
    /// Key format: /v/<collection_id>/<doc_short_id> — a prefix distinct
    /// from doc blobs so blob scans never have to filter version keys.
    pub(crate) fn version_key(&self, doc_short_id: u64) -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(VERSION_KEY_PREFIX);
        key.extend_from_slice(self.def.collection_id.as_bytes());
        key.push(b'/');
        key.extend_from_slice(&encode_doc_short_id(doc_short_id));
        key
    }

    /// Generate the key prefix for iterating collection documents.
    pub(crate) fn collection_key_prefix(&self) -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(DOC_KEY_PREFIX);
        key.extend_from_slice(self.def.collection_id.as_bytes());
        key.push(b'/');
        key
    }

    /// Resolve a public DocID to this collection's doc short ID.
    ///
    /// Returns `None` for unknown documents or documents belonging to a
    /// different collection.
    pub async fn resolve_doc_short_id(
        &self,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<Option<u64>> {
        crate::docid::map::get_doc_short_id(
            systemstore,
            self.resolved_root_id(),
            &doc_id.to_string(),
        )
        .await
    }

    /// Resolve an input DocID or alias to its local short ID and canonical
    /// genesis-derived DocID.
    pub(crate) async fn resolve_doc_identity(
        &self,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<Option<(u64, DocID)>> {
        let Some(doc_short_id) = self.resolve_doc_short_id(systemstore, doc_id).await? else {
            return Ok(None);
        };
        let canonical = crate::docid::map::get_doc_id(systemstore, doc_short_id)
            .await?
            .ok_or_else(|| {
                Error::InvalidDocument(format!(
                    "document short ID {doc_short_id} has no canonical DocID"
                ))
            })?
            .parse::<DocID>()?;
        Ok(Some((doc_short_id, canonical)))
    }

    /// Resolve an input DocID or alias, returning the canonical identity.
    pub(crate) async fn require_doc_identity(
        &self,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<(u64, DocID)> {
        self.resolve_doc_identity(systemstore, doc_id)
            .await?
            .ok_or_else(|| Error::DocumentNotFound(doc_id.to_string()))
    }

    /// Store the schema version for a document.
    pub(crate) async fn store_version(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<()> {
        self.store_document_version(datastore, doc_short_id, &self.def.version_id)
            .await
    }

    /// Validate the version even when its value does not need rewriting.
    /// A concurrent schema transition must invalidate an old-schema writer.
    pub(crate) async fn store_document_version(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        version: &str,
    ) -> Result<()> {
        let key = self.version_key(doc_short_id);
        if datastore
            .get(&key)
            .await
            .map_err(Error::Storage)?
            .as_deref()
            == Some(version.as_bytes())
        {
            return Ok(());
        }
        datastore
            .set(&key, version.as_bytes())
            .await
            .map_err(Error::Storage)
    }

    /// Load the schema version for a document.
    pub(crate) async fn load_version(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<Option<String>> {
        let key = self.version_key(doc_short_id);

        match datastore.get(&key).await.map_err(Error::Storage)? {
            Some(bytes) => {
                let version = String::from_utf8(bytes.into())
                    .map_err(|e| Error::text_decode("invalid version encoding", e))?;
                Ok(Some(version))
            }
            None => Ok(None),
        }
    }

    /// Delete the schema version for a document.
    pub(crate) async fn delete_version(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<()> {
        let key = self.version_key(doc_short_id);
        datastore.delete(&key).await.map_err(Error::Storage)
    }
}

#[cfg(test)]
mod tests {
    use super::{Collection, IndexStatuses};
    use defra_core::ActionStatus;
    use rapidhash::{HashMapExt, RapidHashMap};
    use schema::{CollectionVersion, IndexDescription};
    use std::sync::Arc;

    #[test]
    fn reused_schema_arc_gets_fresh_snapshot_index_statuses() {
        let mut schema = CollectionVersion::new("users", "version", "collection", vec![]);
        let mut index = IndexDescription::new("by_name");
        index.id = 1;
        schema.indexes.push(index);
        let expected = Collection::new(schema);

        let mut actions = RapidHashMap::new();
        actions.insert(1, ActionStatus::IN_PROGRESS);
        let snapshot = Collection::with_index_actions_reusing_schema(
            &expected,
            &IndexStatuses::from_actions(actions),
        );

        assert!(Arc::ptr_eq(&expected.def, &snapshot.def));
        assert_eq!(snapshot.write_indexes().len(), 1);
        assert!(snapshot.queryable_indexes().is_empty());
    }

    #[test]
    fn an_owed_reindex_hides_every_index_from_queries_but_not_from_writes() {
        let mut schema = CollectionVersion::new("users", "version", "collection", vec![]);
        let mut index = IndexDescription::new("by_name");
        index.id = 1;
        schema.indexes.push(index);
        let settled = Collection::new(schema);

        let owed = settled.with_reindex_owed(true);
        assert!(owed.queryable_indexes().is_empty());
        assert!(owed.schema_for_queries().indexes.is_empty());
        assert_eq!(owed.write_indexes().len(), 1);
        assert!(
            owed.same_index_statuses(&settled),
            "a writer's check is not about which indexes answer queries"
        );

        let again = owed.with_reindex_owed(false);
        assert_eq!(again.queryable_indexes().len(), 1);
        assert_eq!(again.schema_for_queries().indexes.len(), 1);
    }
}

pub(crate) mod parts;

pub(crate) mod value;
