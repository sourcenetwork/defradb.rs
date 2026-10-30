/// Index manager for DefraDB collections.
///
/// Handles index lifecycle operations:
/// - Creating new indexes (with ID generation and bulk indexing)
/// - Dropping indexes (with entry cleanup)
/// - Loading index instances from schema
/// - Maintaining indexes during document mutations
pub mod value_extraction;

use crate::index::error::{Error, Result};
use datastore::NamespaceView;
use document::Document;
use rapidhash::{HashMapExt, RapidHashMap};
use schema::{
    CollectionVersion, IndexDescription, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription, VectorIndexDescription,
};
use storage::corekv::Key;
use storage::index::FullTextIndex;

mod deferred;
mod index_type;
mod read;
mod rebuild;
mod unique;
pub use index_type::IndexType;
pub use read::IndexRead;
use storage::keys::IndexIDSequenceKey;

pub mod doc_source;
pub use doc_source::{DocumentSource, SliceSource};

/// Result of a bulk index operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkIndexResult {
    /// Number of documents successfully indexed.
    pub indexed: usize,
    /// Number of documents skipped (e.g., missing document ID).
    pub skipped: usize,
}

/// One batch of [`IndexManager::index_batch_from`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchIndexResult {
    /// Documents indexed by this batch.
    pub indexed: usize,
    /// Documents skipped (unset short id).
    pub skipped: usize,
    /// The short id of the last document indexed, if any.
    pub last_doc_short_id: Option<u64>,
    /// The source yielded nothing more.
    pub exhausted: bool,
}

/// Generate an index name matching Go's `{Col}_{firstField}_ASC` pattern.
///
/// If the base name already exists, appends `_2`, `_3`, etc. to avoid collisions.
fn generate_index_name(
    collection_name: &str,
    first_field: &str,
    existing_names: &[String],
) -> String {
    let base = format!("{}_{}_ASC", collection_name, first_field);
    if !existing_names.contains(&base) {
        return base;
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{}_{}", base, suffix);
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Check if an index name is valid per Go's rules.
///
/// Must start with a letter or underscore, and contain only alphanumeric characters
/// and underscores. Matches Go's `isValidIndexName()`.
fn is_valid_index_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reject the unset doc short ID (0): index maintenance requires a mapped document.
fn require_doc_short_id(doc_short_id: u64) -> Result<()> {
    if doc_short_id == 0 {
        return Err(Error::InvalidDocument(
            "document must have a doc short ID".to_string(),
        ));
    }
    Ok(())
}

/// Generate the internal map key used for full-text indexes.
///
/// This key is intentionally invalid for public index creation APIs because
/// full-text indexes are synthesized from `@fulltext` schema directives and do
/// not share the same namespace as user-defined secondary indexes.
pub fn fulltext_index_name(field_name: &str) -> String {
    format!("__fulltext__:{field_name}")
}

/// Manages indexes for a collection.
///
/// Provides operations for creating, dropping, and maintaining secondary indexes.
/// Indexes are stored in the collection schema and persisted to storage.
pub struct IndexManager {
    /// Collection short ID for index key generation
    collection_short_id: u32,
    /// Collection ID for document/deletion-marker key generation. Unique
    /// enforcement needs it to ask whether the document a stale index entry
    /// points at is still alive (#1111/#700). Empty when constructed without a
    /// schema, in which case stale-entry healing is disabled.
    collection_id: String,
    /// Active index instances keyed by index name
    indexes: RapidHashMap<String, IndexType>,
    /// The node's CRDT types, which say whether an index is deferred.
    crdts: crdt::Crdts,
    /// Where a commit that owes a deferred index asks for its drain.
    drains: Option<crate::index::deferred::DrainRequests>,
}

impl IndexManager {
    fn validate_unique_value_sets(
        index: &IndexType,
        value_sets: &[Vec<document::NormalValue>],
    ) -> Result<()> {
        if !matches!(index, IndexType::Unique(_)) {
            return Ok(());
        }

        for (position, values) in value_sets.iter().enumerate() {
            if values.iter().any(document::NormalValue::is_nil) {
                continue;
            }
            if value_sets[..position].contains(values) {
                return Err(Error::Storage(
                    storage::corekv::Error::UniqueConstraintViolation,
                ));
            }
        }
        Ok(())
    }

    /// Create a new empty IndexManager.
    pub fn new(collection_short_id: u32) -> Self {
        Self {
            collection_short_id,
            collection_id: String::new(),
            indexes: RapidHashMap::new(),
            crdts: crdt::Crdts::default(),
            drains: None,
        }
    }

    /// Asks `drains` for a drain of each deferred index a write owes.
    pub fn with_drains(mut self, drains: Option<&crate::index::deferred::DrainRequests>) -> Self {
        self.drains = drains.cloned();
        self
    }

    /// Lets this manager's DiskANN indexes share their quantizers through
    /// `caches`, so a write does not rebuild one per transaction.
    pub fn with_vector_caches(
        mut self,
        caches: &crate::index::vector::engine::diskann::CodeCaches,
    ) -> Self {
        for index in self.indexes.values_mut() {
            if let IndexType::Vector(vector) = index {
                vector.share_quantizer(caches);
            }
        }
        self
    }

    /// Load indexes from a collection schema.
    ///
    /// # Errors
    ///
    /// Returns an error if any index in the schema has invalid configuration
    /// (e.g., empty fields list).
    pub fn from_collection(
        collection_short_id: u32,
        schema: &CollectionVersion,
        crdts: &crdt::Crdts,
    ) -> Result<Self> {
        Self::from_indexes(collection_short_id, schema, &schema.indexes, crdts)
    }

    /// Load a selected set of regular indexes plus all full-text indexes from a collection schema.
    pub fn from_indexes(
        collection_short_id: u32,
        schema: &CollectionVersion,
        indexes: &[IndexDescription],
        crdts: &crdt::Crdts,
    ) -> Result<Self> {
        let mut manager = Self::new(collection_short_id);
        manager.collection_id = schema.collection_id.clone();
        manager.crdts = crdts.clone();
        for desc in indexes {
            if desc.fields.is_empty() {
                return Err(Error::Other(format!(
                    "index '{}' in schema has no fields",
                    desc.name
                )));
            }
            // `try_new` rather than `new`: a stored vector description that
            // cannot be built must say so, not quietly load as an ordered index
            // over the raw vector field.
            let index = IndexType::try_new(collection_short_id, desc.clone())?;
            manager.indexes.insert(desc.name.clone(), index);
        }
        // Load full-text indexes.
        // IDs are derived deterministically from the field name to avoid collisions
        // with regular indexes (which use the IndexIDSequenceKey mechanism).
        // The high-bit (0x8000_0000) separates the namespace from regular index IDs.
        for ft_desc in &schema.fulltext_indexes {
            let idx_name = fulltext_index_name(&ft_desc.field_name);
            let idx_id = {
                // FNV-1a: stable across Rust versions unlike DefaultHasher
                let mut h: u32 = 2166136261;
                for byte in ft_desc.field_name.as_bytes() {
                    h ^= *byte as u32;
                    h = h.wrapping_mul(16777619);
                }
                h | 0x8000_0000
            };
            let desc = IndexDescription {
                name: idx_name.clone(),
                id: idx_id,
                fields: vec![IndexedFieldDescription {
                    name: ft_desc.field_name.clone(),
                    descending: false,
                }],
                unique: false,
                kind: None,
                auto_generated: false,
            };
            let ft_index = FullTextIndex::new(collection_short_id, desc, ft_desc.clone());
            manager
                .indexes
                .insert(idx_name, IndexType::FullText(ft_index));
        }
        Ok(manager)
    }

    /// Get the collection short ID.
    pub fn collection_short_id(&self) -> u32 {
        self.collection_short_id
    }

    /// Get all index descriptions.
    pub fn get_indexes(&self) -> Vec<&IndexDescription> {
        self.indexes.values().map(|idx| idx.description()).collect()
    }

    /// Get an index by name.
    pub fn get_index(&self, name: &str) -> Option<&IndexType> {
        self.indexes.get(name)
    }

    /// Check if an index exists.
    pub fn has_index(&self, name: &str) -> bool {
        self.indexes.contains_key(name)
    }

    /// Create a new index.
    ///
    /// This creates the index definition but does NOT bulk-index existing documents.
    /// Call `bulk_index` separately to index existing documents.
    ///
    /// Returns the IndexDescription with the generated ID.
    pub async fn create_index(
        &mut self,
        datastore: &NamespaceView,
        collection_name: &str,
        name: String,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
    ) -> Result<IndexDescription> {
        self.create_index_of_kind(
            datastore,
            collection_name,
            name,
            fields,
            IndexKind::Ordered(OrderedIndexDescription { unique }),
        )
        .await
    }

    /// The index kind a create-index request asks for.
    ///
    /// A vector index covers exactly one field and is never unique, which is
    /// why [`IndexKind::Vector`] has no `unique` to carry. A request that pairs
    /// a vector config with `unique`, or with several fields, is therefore
    /// refused here rather than having one of the two quietly dropped: the
    /// first would build an index that does not enforce what was asked for, the
    /// second would index a different column than the request named.
    ///
    /// Static because every entry point (the CLI, the HTTP API, and any
    /// embedder) has to make the same call, and a second copy of this decision
    /// is a divergence waiting to ship.
    pub fn requested_kind(
        fields: &[IndexedFieldDescription],
        unique: bool,
        vector: Option<VectorIndexDescription>,
    ) -> Result<IndexKind> {
        let Some(vector) = vector else {
            return Ok(IndexKind::Ordered(OrderedIndexDescription { unique }));
        };
        if unique {
            return Err(Error::Other("vector index cannot be unique".to_string()));
        }
        if fields.len() != 1 {
            return Err(Error::Other(format!(
                "vector index must be on exactly one field, got {}",
                fields.len()
            )));
        }
        Ok(IndexKind::Vector(vector))
    }

    /// Creates an index of any kind.
    ///
    /// [`create_index`](Self::create_index) is this with an ordered kind. The
    /// kind is a single argument rather than a `unique` flag plus a config,
    /// because those two can disagree and the resulting index would be in a
    /// state that means nothing.
    pub async fn create_index_of_kind(
        &mut self,
        datastore: &NamespaceView,
        collection_name: &str,
        name: String,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
    ) -> Result<IndexDescription> {
        let name = if name.is_empty() {
            let first_field = fields.first().map(|f| f.name.as_str()).unwrap_or("unknown");
            let existing: Vec<String> = self.indexes.keys().cloned().collect();
            generate_index_name(collection_name, first_field, &existing)
        } else {
            name
        };

        if !is_valid_index_name(&name) {
            return Err(Error::Other(format!(
                "index with invalid name. Name: {}",
                name
            )));
        }

        if self.indexes.contains_key(&name) {
            return Err(Error::Other(format!(
                "index with name already exists. Name: {}",
                name
            )));
        }

        if fields.is_empty() {
            return Err(Error::Other(
                "index must have at least one field".to_string(),
            ));
        }

        if let IndexKind::Vector(vector) = &kind {
            let errs = vector.creation_errors(&name);
            if !errs.is_empty() {
                return Err(Error::Other(errs.join("; ")));
            }
        }

        let index_id = self.next_index_id(datastore).await?;

        let desc = IndexDescription {
            name: name.clone(),
            id: index_id,
            fields,
            unique: false,
            kind: Some(kind),
            auto_generated: false,
        }
        .normalized();

        // `try_new`, so a vector index with out-of-range parameters is refused
        // here rather than becoming an ordered index over the vector field.
        let index = IndexType::try_new(self.collection_short_id, desc.clone())?;
        self.indexes.insert(name, index);

        Ok(desc)
    }

    /// Drop an index by name.
    ///
    /// Removes the index from the manager and clears all index entries from storage.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the index existed and was dropped successfully.
    /// - `Ok(false)` if the index did not exist (idempotent - not an error).
    /// - `Err(...)` only on storage failures during entry cleanup.
    ///
    /// # Idempotency
    ///
    /// This method is intentionally idempotent. Calling it multiple times with
    /// the same index name is safe and will not produce errors. This matches
    /// SQL `DELETE INDEX IF EXISTS` semantics.
    pub async fn delete_index(&mut self, datastore: &NamespaceView, name: &str) -> Result<bool> {
        match self.indexes.remove(name) {
            Some(index) => {
                index
                    .remove_all(&mut datastore.clone())
                    .await
                    .map_err(Error::Storage)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Get the next available index ID for this collection.
    ///
    /// # Concurrency
    ///
    /// This method assumes single-writer semantics provided by the transaction layer.
    /// The read-modify-write sequence is NOT atomic. Concurrent calls from different
    /// transactions could generate duplicate IDs. Callers must ensure exclusive access
    /// to index creation within a collection, either through transaction isolation or
    /// external synchronization.
    async fn next_index_id(&self, datastore: &NamespaceView) -> Result<u32> {
        let counter = index_id_floor(datastore, self.collection_short_id).await?;
        // The schema is the authority on which ids exist: a counter advanced
        // through another store, or lost, must never hand out an id an index
        // already owns.
        let highest = self
            .indexes
            .values()
            .map(|index| index.description().id)
            .max()
            .unwrap_or(0);

        let next_id = counter.max(highest) + 1;
        datastore
            .set(
                &IndexIDSequenceKey::new(self.collection_short_id.to_string()).bytes(),
                &next_id.to_be_bytes(),
            )
            .await
            .map_err(Error::Storage)?;

        Ok(next_id)
    }

    /// Bulk index all existing documents in a collection.
    ///
    /// This should be called after creating a new index to populate it with
    /// existing document data. Each document is paired with its node-local
    /// doc short ID.
    ///
    /// # Returns
    ///
    /// Returns a `BulkIndexResult` containing:
    /// - `indexed`: Number of documents successfully indexed.
    /// - `skipped`: Number of documents skipped (unset short ID).
    pub async fn bulk_index(
        &self,
        datastore: &NamespaceView,
        index_name: &str,
        documents: &[(u64, Document)],
        schema: &CollectionVersion,
    ) -> Result<BulkIndexResult> {
        let mut source = SliceSource::new(documents);
        self.bulk_index_from(datastore, index_name, &mut source, schema)
            .await
    }

    /// [`bulk_index`](Self::bulk_index) over a pull-based source, holding one
    /// document at a time rather than the whole collection.
    pub async fn bulk_index_from<S: DocumentSource + ?Sized>(
        &self,
        datastore: &NamespaceView,
        index_name: &str,
        source: &mut S,
        schema: &CollectionVersion,
    ) -> Result<BulkIndexResult> {
        let batch = self
            .index_batch_from(datastore, index_name, source, schema, usize::MAX)
            .await?;
        self.build_index(datastore, index_name).await?;
        Ok(BulkIndexResult {
            indexed: batch.indexed,
            skipped: batch.skipped,
        })
    }

    /// Index up to `max_docs` documents from `source`, without building.
    ///
    /// An entry the document already holds is left as it is: a write that
    /// landed between the definition and this batch maintained the index
    /// itself, and a batch re-run after a conflict meets its own work.
    pub async fn index_batch_from<S: DocumentSource + ?Sized>(
        &self,
        datastore: &NamespaceView,
        index_name: &str,
        source: &mut S,
        schema: &CollectionVersion,
        max_docs: usize,
    ) -> Result<BatchIndexResult> {
        let index = self.index_named(index_name)?;
        let mut batch = BatchIndexResult {
            indexed: 0,
            skipped: 0,
            last_doc_short_id: None,
            exhausted: false,
        };
        let mut mutable_datastore = datastore.clone();

        while batch.indexed + batch.skipped < max_docs {
            let Some((doc_short_id, doc)) = source.next().await? else {
                batch.exhausted = true;
                break;
            };
            if doc_short_id == 0 {
                batch.skipped += 1;
                continue;
            }
            let value_sets = self.extract_index_values(&doc, index.description(), schema)?;
            if self.is_deferred(index.description(), schema) {
                self.backfill(datastore, index, doc_short_id, &value_sets)
                    .await?;
            } else {
                for values in &value_sets {
                    self.save_healing_stale_unique(
                        &mut mutable_datastore,
                        index,
                        doc_short_id,
                        values,
                    )
                    .await
                    .map_err(Error::Storage)?;
                }
            }
            batch.indexed += 1;
            batch.last_doc_short_id = Some(doc_short_id);
        }
        Ok(batch)
    }

    /// One chance to train and build, taken once a bulk load has landed
    /// rather than asked per document. A no-op for every index kind but
    /// vector.
    pub async fn build_index(&self, datastore: &NamespaceView, index_name: &str) -> Result<()> {
        self.index_named(index_name)?
            .build_if_needed(&mut datastore.clone())
            .await
            .map_err(Error::Storage)
    }

    fn index_named(&self, index_name: &str) -> Result<&IndexType> {
        self.indexes
            .get(index_name)
            .ok_or_else(|| Error::Other(format!("index '{}' not found", index_name)))
    }

    /// Merge-path variant of [`Self::on_document_create`]: resolves live
    /// unique conflicts deterministically instead of erroring (#1111).
    pub async fn on_document_create_merge(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
    ) -> Result<Vec<events::UniqueIndexConflictData>> {
        require_doc_short_id(doc_short_id)?;
        let doc_id = doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("document must have an ID".to_string()))?
            .to_string();
        let mut mutable_datastore = datastore.clone();
        let mut conflicts = Vec::new();
        for index in self.indexes.values() {
            let value_sets = self.extract_index_values(doc, index.description(), schema)?;
            if self.is_deferred(index.description(), schema) {
                self.place(datastore, index, doc_short_id, &value_sets)
                    .await?;
                continue;
            }
            for values in &value_sets {
                conflicts.extend(
                    self.save_resolving_unique_conflict(
                        &mut mutable_datastore,
                        systemstore,
                        index,
                        doc_short_id,
                        &doc_id,
                        values,
                    )
                    .await
                    .map_err(Error::Storage)?,
                );
            }
        }
        Ok(conflicts)
    }

    /// Merge-path variant of [`Self::on_document_update`]: resolves live
    /// unique conflicts deterministically instead of erroring (#1111).
    pub async fn on_document_update_merge(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        old_doc: &Document,
        new_doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
    ) -> Result<Vec<events::UniqueIndexConflictData>> {
        require_doc_short_id(doc_short_id)?;
        let doc_id = new_doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("document must have an ID".to_string()))?
            .to_string();
        let mut mutable_datastore = datastore.clone();
        let mut conflicts = Vec::new();
        for index in self.indexes.values() {
            // Owed without comparing: a merge that wrote the field before this
            // runs can read the same value as old and new.
            if self.is_deferred(index.description(), schema) {
                self.owe(datastore, index.description(), doc_short_id)
                    .await?;
                continue;
            }
            let old_value_sets = self.extract_index_values(old_doc, index.description(), schema)?;
            let new_value_sets = self.extract_index_values(new_doc, index.description(), schema)?;
            if old_value_sets != new_value_sets {
                for old_values in &old_value_sets {
                    index
                        .delete(&mut mutable_datastore, doc_short_id, old_values)
                        .await
                        .map_err(Error::Storage)?;
                }
                for new_values in &new_value_sets {
                    conflicts.extend(
                        self.save_resolving_unique_conflict(
                            &mut mutable_datastore,
                            systemstore,
                            index,
                            doc_short_id,
                            &doc_id,
                            new_values,
                        )
                        .await
                        .map_err(Error::Storage)?,
                    );
                }
            }
        }
        Ok(conflicts)
    }

    /// Update indexes when a document is created.
    pub async fn on_document_create(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
    ) -> Result<()> {
        require_doc_short_id(doc_short_id)?;

        let mut mutable_datastore = datastore.clone();

        for index in self.indexes.values() {
            let value_sets = self.extract_index_values(doc, index.description(), schema)?;
            if self.is_deferred(index.description(), schema) {
                self.place(datastore, index, doc_short_id, &value_sets)
                    .await?;
                continue;
            }
            Self::validate_unique_value_sets(index, &value_sets)?;
            for values in &value_sets {
                // Local creates stay strict but self-heal stale unique entries
                // pointing at a deleted or missing document (#1111/#700).
                self.save_healing_stale_unique(&mut mutable_datastore, index, doc_short_id, values)
                    .await
                    .map_err(Error::Storage)?;
            }
        }

        Ok(())
    }

    /// Update indexes when a document is updated.
    ///
    /// For array fields, this deletes all old index entries and creates new ones.
    /// The update is performed by deleting all old value combinations and saving
    /// all new value combinations.
    pub async fn on_document_update(
        &self,
        datastore: &NamespaceView,
        old_doc: &Document,
        new_doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
    ) -> Result<()> {
        require_doc_short_id(doc_short_id)?;

        let mut mutable_datastore = datastore.clone();

        for index in self.indexes.values() {
            if self.is_deferred(index.description(), schema) {
                self.owe(datastore, index.description(), doc_short_id)
                    .await?;
                continue;
            }
            let old_value_sets = self.extract_index_values(old_doc, index.description(), schema)?;
            let new_value_sets = self.extract_index_values(new_doc, index.description(), schema)?;

            if old_value_sets != new_value_sets {
                Self::validate_unique_value_sets(index, &new_value_sets)?;
                for old_values in &old_value_sets {
                    index
                        .delete(&mut mutable_datastore, doc_short_id, old_values)
                        .await
                        .map_err(Error::Storage)?;
                }
                for new_values in &new_value_sets {
                    self.save_healing_stale_unique(
                        &mut mutable_datastore,
                        index,
                        doc_short_id,
                        new_values,
                    )
                    .await
                    .map_err(Error::Storage)?;
                }
            }
        }

        Ok(())
    }

    /// Update only indexes whose fields intersect a known mutation patch.
    /// The caller must provide old/new values for every field in each touched
    /// (including composite) index.
    pub(crate) async fn on_document_update_modified(
        &self,
        datastore: &NamespaceView,
        old_doc: &Document,
        new_doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
        modified: &rapidhash::RapidHashSet<String>,
    ) -> Result<()> {
        require_doc_short_id(doc_short_id)?;
        let mut mutable_datastore = datastore.clone();
        for index in self.indexes.values() {
            if !index
                .description()
                .fields
                .iter()
                .any(|field| modified.contains(&field.name))
            {
                continue;
            }
            // The write read nothing it could compare: a blind increment
            // leaves its old value unvalidated (`update_parts`).
            if self.is_deferred(index.description(), schema) {
                self.owe(datastore, index.description(), doc_short_id)
                    .await?;
                continue;
            }
            let old_value_sets = self.extract_index_values(old_doc, index.description(), schema)?;
            let new_value_sets = self.extract_index_values(new_doc, index.description(), schema)?;
            if old_value_sets == new_value_sets {
                continue;
            }
            Self::validate_unique_value_sets(index, &new_value_sets)?;
            for old_values in &old_value_sets {
                index
                    .delete(&mut mutable_datastore, doc_short_id, old_values)
                    .await
                    .map_err(Error::Storage)?;
            }
            for new_values in &new_value_sets {
                self.save_healing_stale_unique(
                    &mut mutable_datastore,
                    index,
                    doc_short_id,
                    new_values,
                )
                .await
                .map_err(Error::Storage)?;
            }
        }
        Ok(())
    }

    /// Update indexes when a document is deleted.
    pub async fn on_document_delete(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        schema: &CollectionVersion,
    ) -> Result<()> {
        require_doc_short_id(doc_short_id)?;

        let mut mutable_datastore = datastore.clone();

        for index in self.indexes.values() {
            if self.is_deferred(index.description(), schema) {
                self.owe(datastore, index.description(), doc_short_id)
                    .await?;
                continue;
            }
            let value_sets = self.extract_index_values(doc, index.description(), schema)?;
            for values in &value_sets {
                index
                    .delete(&mut mutable_datastore, doc_short_id, values)
                    .await
                    .map_err(Error::Storage)?;
            }
        }

        Ok(())
    }

    /// Get all index names.
    pub fn index_names(&self) -> Vec<&str> {
        self.indexes.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of indexes.
    pub fn index_count(&self) -> usize {
        self.indexes.len()
    }
}

/// The highest index id the collection with `collection_short_id` has ever
/// handed out or retired: a new index id is always above it.
pub(crate) async fn index_id_floor(
    datastore: &NamespaceView,
    collection_short_id: u32,
) -> Result<u32> {
    let key = IndexIDSequenceKey::new(collection_short_id.to_string());
    Ok(
        match datastore.get(&key.bytes()).await.map_err(Error::Storage)? {
            Some(bytes) if bytes.len() == 4 => {
                u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            }
            _ => 0,
        },
    )
}

/// Keep `index_id` from ever being handed out again on the collection with
/// `collection_short_id`. A dropped index's entries are deleted after its
/// drop commits, and an entry key names only the collection and the index
/// id, so a new index with the same id would lose its entries to that
/// deletion.
pub(crate) async fn retire_index_id(
    datastore: &NamespaceView,
    collection_short_id: u32,
    index_id: u32,
) -> Result<()> {
    if index_id_floor(datastore, collection_short_id).await? >= index_id {
        return Ok(());
    }
    datastore
        .set(
            &IndexIDSequenceKey::new(collection_short_id.to_string()).bytes(),
            &index_id.to_be_bytes(),
        )
        .await
        .map_err(Error::Storage)
}
