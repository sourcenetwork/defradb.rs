use crdt::stored::StoredDocument;
use storage::keys::doc_id_index::decode_doc_short_id;

use super::*;

impl Collection {
    /// Read all schema fields used to materialize a document. Until the value
    /// merge operator is installed, whole-value writes still validate the blob.
    pub(crate) async fn read_document_blob(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<Option<bytes::Bytes>> {
        self.read_document_blob_and_version(datastore, doc_short_id, false)
            .await
            .map(|(data, _)| data)
    }

    async fn read_document_blob_and_version(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        full_value_read: bool,
    ) -> Result<(Option<bytes::Bytes>, Option<String>)> {
        let key = self.doc_key(doc_short_id);
        // Writable callers retain the all-declared-fields projection so the
        // engine validates schema part identities. A read-only full-document
        // fetch can use the whole key directly because it needs every field
        // and does not commit a dependency set. A schema transition replaces
        // the whole value, but a full read works for current and historical
        // versions.
        let version = self.load_version(datastore, doc_short_id).await?;
        let data = if full_value_read {
            datastore.get(&key).await
        } else {
            let parts = self
                .part_ids_of(
                    datastore,
                    self.def.fields.iter().map(|field| field.name.as_str()),
                )
                .await?;
            match parts {
                Some(parts)
                    if !parts.is_empty() && version.as_deref() == Some(self.version_id()) =>
                {
                    datastore.get_parts(&key, &parts).await
                }
                _ => datastore.get(&key).await,
            }
        }
        .map_err(Error::Storage)?;
        Ok((data, version))
    }

    /// Read the complete document while validating only the named field parts
    /// when the stored schema version is current; with no fields, nothing is
    /// validated but the deletion marker. A lens transition needs the
    /// conservative whole-blob dependency.
    pub(crate) async fn get_with_datastore_fields(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
        fields: &rapidhash::RapidHashSet<String>,
    ) -> Result<Option<Document>> {
        let version = self.load_version(datastore, doc_short_id).await?;
        self.get_with_datastore_fields_at(datastore, doc_short_id, doc_id, fields, version)
            .await
    }

    /// [`get_with_datastore_fields`](Self::get_with_datastore_fields) for a
    /// caller that has already read the document's stored `version`.
    pub(crate) async fn get_with_datastore_fields_at(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
        fields: &rapidhash::RapidHashSet<String>,
        version: Option<String>,
    ) -> Result<Option<Document>> {
        let deleted_key = self.deleted_key(doc_short_id);
        if datastore.has(&deleted_key).await.map_err(Error::Storage)? {
            return Ok(None);
        }
        let parts = self
            .part_ids_of(datastore, fields.iter().map(String::as_str))
            .await?;
        let key = self.doc_key(doc_short_id);
        let whole = |data: Option<bytes::Bytes>| {
            data.map(|data| crdt::stored::document_from_storage(&data, self.schema_arc()))
                .transpose()
        };
        // No fields is a read no decision rests on, never validated.
        let doc = match (version.as_deref() == Some(self.version_id()), parts) {
            (true, Some(parts)) => {
                let projected = datastore
                    .get_parts(&key, &parts)
                    .await
                    .map_err(Error::Storage)?;
                // A legacy or structural whole-value write is not a part map.
                // Keep its full dependency and let the caller rebase every field.
                // A part map is decoded once: every field of one that decodes
                // materializes.
                match projected {
                    Some(bytes) => match StoredDocument::decode(&bytes)
                        .and_then(|stored| stored.materialize(self.schema_arc()))
                    {
                        Some(doc) => Some(doc),
                        None => whole(datastore.get(&key).await.map_err(Error::Storage)?)?,
                    },
                    None => None,
                }
            }
            _ => whole(datastore.get(&key).await.map_err(Error::Storage)?)?,
        };
        let Some(mut doc) = doc else { return Ok(None) };
        doc.set_id(doc_id.clone());
        if let Some(version) = version {
            doc.set_schema_version_id(version);
        }
        Ok(Some(doc))
    }

    /// Get a document by its public DocID, resolving the short ID through
    /// the systemstore mapping. Returns None for unknown or deleted docs.
    pub async fn get_by_doc_id(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<Option<Document>> {
        let Some((doc_short_id, canonical_doc_id)) =
            self.resolve_doc_identity(systemstore, doc_id).await?
        else {
            return Ok(None);
        };
        self.get_with_datastore(datastore, doc_short_id, &canonical_doc_id)
            .await
    }

    /// Get a complete document on a read-only snapshot without loading the
    /// field-part map used by projected reads. Writable callers keep using
    /// `get_by_doc_id`, which validates the declared field parts separately.
    pub(crate) async fn get_by_doc_id_read_only(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<Option<Document>> {
        let Some((doc_short_id, canonical_doc_id)) =
            self.resolve_doc_identity(systemstore, doc_id).await?
        else {
            return Ok(None);
        };
        self.get_with_datastore_full_value(datastore, doc_short_id, &canonical_doc_id)
            .await
    }

    /// Check whether a document exists (and is not deleted) by its public
    /// DocID, resolving the short ID through the systemstore mapping.
    pub async fn exists_by_doc_id(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_id: &DocID,
    ) -> Result<bool> {
        let Some(doc_short_id) = self.resolve_doc_short_id(systemstore, doc_id).await? else {
            return Ok(false);
        };
        self.exists_with_datastore(datastore, doc_short_id).await
    }

    /// Get a document by short ID using a NamespaceView directly.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    /// The returned document carries `doc_id` and its schema version.
    /// Returns None if the document doesn't exist or is deleted.
    pub async fn get_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
    ) -> Result<Option<Document>> {
        // Check if document is deleted
        let deleted_key = self.deleted_key(doc_short_id);
        if datastore.has(&deleted_key).await.map_err(Error::Storage)? {
            return Ok(None);
        }

        self.get_with_datastore_include_deleted(datastore, doc_short_id, doc_id, false)
            .await
            .map(|opt| opt.map(|(doc, _)| doc))
    }

    /// Get a document by short ID with its deletion status.
    ///
    /// Returns (Document, is_deleted) if document exists, None otherwise.
    pub async fn get_with_datastore_include_deleted(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
        check_deleted: bool,
    ) -> Result<Option<(Document, bool)>> {
        self.get_with_datastore_include_deleted_impl(
            datastore,
            doc_short_id,
            doc_id,
            check_deleted,
            false,
        )
        .await
    }

    async fn get_with_datastore_full_value(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
    ) -> Result<Option<Document>> {
        self.get_with_datastore_include_deleted_impl(datastore, doc_short_id, doc_id, false, true)
            .await
            .map(|result| result.map(|(doc, _)| doc))
    }

    async fn get_with_datastore_include_deleted_impl(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
        check_deleted: bool,
        full_value_read: bool,
    ) -> Result<Option<(Document, bool)>> {
        let (data, version) = self
            .read_document_blob_and_version(datastore, doc_short_id, full_value_read)
            .await?;

        match data {
            Some(bytes) => {
                let mut doc = crdt::stored::document_from_storage(&bytes, self.schema_arc())?;
                // The blob stores values only; identity lives in the mapping.
                doc.set_id(doc_id.clone());

                // Load and set the schema version
                if let Some(version) = version {
                    doc.set_schema_version_id(version);
                }

                // Check deletion status if requested
                let is_deleted = if check_deleted {
                    let deleted_key = self.deleted_key(doc_short_id);
                    datastore.has(&deleted_key).await.map_err(Error::Storage)?
                } else {
                    false
                };

                Ok(Some((doc, is_deleted)))
            }
            None => Ok(None),
        }
    }

    /// Get all documents in the collection using a NamespaceView directly.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    /// Each returned document will have its schema version set if stored.
    /// Excludes deleted documents.
    pub async fn get_all_with_datastore(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
    ) -> Result<Vec<Document>> {
        let result = self
            .get_all_with_datastore_include_deleted(datastore, systemstore, false)
            .await?;
        Ok(result.into_iter().map(|(doc, _)| doc).collect())
    }

    /// Get all documents in the collection with deletion status.
    ///
    /// If `show_deleted` is true, returns all documents including deleted ones.
    /// If `show_deleted` is false, returns only non-deleted documents.
    /// Returns tuples of (Document, is_deleted).
    pub async fn get_all_with_datastore_include_deleted(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        show_deleted: bool,
    ) -> Result<Vec<(Document, bool)>> {
        let entries = self
            .get_all_with_datastore_short_ids(datastore, systemstore, show_deleted)
            .await?;
        Ok(entries
            .into_iter()
            .map(|(_, doc, is_deleted)| (doc, is_deleted))
            .collect())
    }

    /// Get all documents in the collection with their doc short IDs and
    /// deletion status.
    ///
    /// Blobs are keyed by doc short ID, so iteration yields documents in
    /// allocation (insertion) order — matching Go v1.0.0's default result
    /// ordering. Public DocIDs are resolved through the systemstore mapping.
    ///
    /// Returns tuples of (doc_short_id, Document, is_deleted).
    pub async fn get_all_with_datastore_short_ids(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        show_deleted: bool,
    ) -> Result<Vec<(u64, Document, bool)>> {
        self.scan_documents(datastore, systemstore, show_deleted, None)
            .await
    }

    /// Every live document, for an update whose decision rests only on
    /// `fields`: a document this transaction later merges into is validated
    /// over those parts alone, so a write of another field does not conflict.
    /// A version without part IDs scans as [`Self::get_all_with_datastore`].
    pub(crate) async fn get_all_with_datastore_deciding_on(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        fields: &rapidhash::RapidHashSet<String>,
    ) -> Result<Vec<Document>> {
        let parts = self
            .part_ids_of(datastore, fields.iter().map(String::as_str))
            .await?;
        let entries = self
            .scan_documents(
                datastore,
                systemstore,
                false,
                parts.map(Vec::into_boxed_slice),
            )
            .await?;
        Ok(entries.into_iter().map(|(_, doc, _)| doc).collect())
    }

    async fn scan_documents(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        show_deleted: bool,
        parts: Option<Box<[u32]>>,
    ) -> Result<Vec<(u64, Document, bool)>> {
        let prefix = self.collection_key_prefix();
        let prefix_len = prefix.len();
        let opts = IterOptions::new().with_prefix(prefix).with_parts(parts);

        let mut iter = datastore.iterator(opts).await.map_err(Error::Storage)?;

        let mut entries: Vec<(u64, Document)> = Vec::new();
        while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
            let Ok(doc_short_id) = decode_doc_short_id(&pair.key[prefix_len..]) else {
                continue;
            };

            let doc = crdt::stored::document_from_storage(&pair.value, self.schema_arc())
                .map_err(|e| Error::document_at_key(&pair.key, e))?;
            entries.push((doc_short_id, doc));
        }
        iter.close().await.map_err(Error::Storage)?;

        let mut docs = Vec::new();
        for (doc_short_id, doc) in entries {
            if let Some(hydrated) = self
                .hydrate(datastore, systemstore, doc_short_id, doc, show_deleted)
                .await?
            {
                docs.push(hydrated);
            }
        }

        Ok(docs)
    }

    /// Reads the given documents by short id, without scanning the collection.
    ///
    /// One point read per id, so the cost is the number asked for rather than
    /// the size of the collection. That is what makes a query that already
    /// knows which documents it wants, such as one narrowed by a vector index,
    /// cheaper than the full scan it replaces.
    ///
    /// Ids that are absent are skipped rather than reported: a caller holding
    /// an id from an index may be holding one whose document has since gone.
    /// The result follows the order of `doc_short_ids`.
    pub async fn get_by_short_ids(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> Result<Vec<(u64, Document, bool)>> {
        let mut docs = Vec::with_capacity(doc_short_ids.len());
        for &doc_short_id in doc_short_ids {
            let Some(bytes) = datastore
                .get(&self.doc_key(doc_short_id))
                .await
                .map_err(Error::Storage)?
            else {
                continue;
            };
            let doc = crdt::stored::document_from_storage(&bytes, self.schema_arc())
                .map_err(|e| Error::document_at_key(&self.doc_key(doc_short_id), e))?;
            if let Some(hydrated) = self
                .hydrate(datastore, systemstore, doc_short_id, doc, show_deleted)
                .await?
            {
                docs.push(hydrated);
            }
        }
        Ok(docs)
    }

    /// Completes a document read from its blob: its id, its deletion state and
    /// its schema version. `None` when it should not be returned at all.
    ///
    /// Shared by the scanning and the point-read paths so the two cannot
    /// disagree about what a loaded document looks like.
    async fn hydrate(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        doc_short_id: u64,
        mut doc: Document,
        show_deleted: bool,
    ) -> Result<Option<(u64, Document, bool)>> {
        let Some(doc_id_str) = crate::docid::map::get_doc_id(systemstore, doc_short_id).await?
        else {
            return Ok(None);
        };
        let Ok(doc_id) = doc_id_str.parse::<DocID>() else {
            return Ok(None);
        };
        doc.set_id(doc_id);

        let is_deleted = self.is_deleted(datastore, doc_short_id).await?;
        if is_deleted && !show_deleted {
            return Ok(None);
        }

        if let Some(version) = self.load_version(datastore, doc_short_id).await? {
            doc.set_schema_version_id(version);
        }

        Ok(Some((doc_short_id, doc, is_deleted)))
    }

    /// Create a new document blob using a NamespaceView directly.
    ///
    /// The document must already carry its (genesis-CID-derived) DocID and
    /// the caller supplies the allocated short ID. Duplicate detection is
    /// the caller's responsibility via the DocID mapping.
    pub async fn create_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
    ) -> Result<DocID> {
        // Validate document against schema
        self.validate_document(doc)?;

        let doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| Error::InvalidDocument("Document must have an ID".into()))?;

        self.claim_series(datastore, doc).await?;

        self.put_document_value(datastore, doc_short_id, doc, true, None)
            .await?;

        // Store schema version
        self.store_version(datastore, doc_short_id).await?;

        Ok(doc_id)
    }

    /// Update an existing document using a NamespaceView directly.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    pub async fn update_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
    ) -> Result<()> {
        // Validate document against schema
        self.validate_document(doc)?;

        let doc_id = doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("Document must have an ID".into()))?;

        if self.is_deleted(datastore, doc_short_id).await? {
            return Err(Error::DocumentNotFound(doc_id.to_string()));
        }
        let old_doc = match self.read_document_blob(datastore, doc_short_id).await? {
            Some(bytes) => {
                let mut d = crdt::stored::document_from_storage(&bytes, self.schema_arc())?;
                d.set_id(doc_id.clone());
                d
            }
            None => return Err(Error::DocumentNotFound(doc_id.to_string())),
        };

        self.validate_immutable_fields_unchanged(&old_doc, doc)?;

        self.release_series(datastore, &old_doc).await?;
        self.claim_series(datastore, doc).await?;

        self.put_document_value(datastore, doc_short_id, doc, false, None)
            .await?;

        // Update schema version to current collection version
        self.store_version(datastore, doc_short_id).await?;

        Ok(())
    }

    /// Delete a document blob using a NamespaceView directly.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    /// Also deletes the document's schema version.
    pub async fn delete_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<bool> {
        let key = self.doc_key(doc_short_id);

        // Check if document exists
        if !datastore.has(&key).await.map_err(Error::Storage)? {
            return Ok(false);
        }

        if self.def.downsample_source.is_some() {
            let systemstore = datastore.sibling(storage::namespace::Namespace::Systemstore);
            if let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id).await? {
                let doc_id = DocID::from_string(&doc_id)?;
                if let Some((doc, _)) = self
                    .get_with_datastore_include_deleted(datastore, doc_short_id, &doc_id, false)
                    .await?
                {
                    self.release_series(datastore, &doc).await?;
                }
            }
        }

        // Delete document
        datastore.delete(&key).await.map_err(Error::Storage)?;

        // Delete schema version
        self.delete_version(datastore, doc_short_id).await?;

        Ok(true)
    }

    /// Check if a document exists and is not deleted.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    pub async fn exists_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<bool> {
        let key = self.doc_key(doc_short_id);
        if !datastore.has(&key).await.map_err(Error::Storage)? {
            return Ok(false);
        }
        // Document exists, check if it's deleted
        let deleted_key = self.deleted_key(doc_short_id);
        let is_deleted = datastore.has(&deleted_key).await.map_err(Error::Storage)?;
        Ok(!is_deleted)
    }

    /// Check if a document exists (regardless of deletion status).
    pub async fn exists_with_datastore_include_deleted(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<bool> {
        let key = self.doc_key(doc_short_id);
        datastore.has(&key).await.map_err(Error::Storage)
    }

    /// Save a document (create or update) using a NamespaceView directly.
    ///
    /// This method takes `NamespaceView` instead of `&DbTxn` to allow
    /// use in async contexts where `Send` futures are required.
    ///
    /// Unlike `create_with_datastore`, this performs an upsert - it will
    /// create the document if it doesn't exist, or update it if it does.
    ///
    /// Note: Validation is skipped for P2P-synced documents since they
    /// may have been created with a different schema version. The document's
    /// schema version is preserved if set, otherwise the collection's version is used.
    pub async fn save_with_datastore(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
    ) -> Result<DocID> {
        self.save_with_datastore_priorities(datastore, doc, doc_short_id, None)
            .await
    }

    pub(crate) async fn save_with_datastore_priorities(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        priorities: Option<&rapidhash::RapidHashMap<String, super::value::FieldOrder>>,
    ) -> Result<DocID> {
        let target = if let Some(version) = doc
            .schema_version_id()
            .filter(|version| *version != self.version_id())
        {
            let system = datastore.sibling(storage::namespace::Namespace::Systemstore);
            crate::definition::loader::get_collection_by_version_id(&system, version)
                .await?
                .map(Collection::new)
        } else {
            None
        };
        let value_collection = target.as_ref().unwrap_or(self);
        let doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| Error::InvalidDocument("Document must have an ID".into()))?;

        if !self.is_deleted(datastore, doc_short_id).await? {
            self.claim_series(datastore, doc).await?;
        }

        if target.is_none()
            && doc
                .schema_version_id()
                .is_some_and(|version| version != self.version_id())
        {
            // An as-yet unknown definition has no local part IDs. A plain put
            // stays whole-value validated until that definition can be loaded.
            datastore
                .set(&self.doc_key(doc_short_id), &doc.to_cbor()?)
                .await
                .map_err(Error::Storage)?;
        } else {
            let merged = if let Some(priorities) = priorities {
                value_collection
                    .merge_resolved_document_value(datastore, doc_short_id, doc, priorities)
                    .await?
            } else {
                false
            };
            if !merged {
                value_collection
                    .put_document_value(datastore, doc_short_id, doc, false, priorities)
                    .await?;
            }
        }

        // Store schema version - preserve document's version if set, otherwise use collection's
        let version = doc.schema_version_id().unwrap_or(&self.def.version_id);
        self.store_document_version(datastore, doc_short_id, version)
            .await?;

        Ok(doc_id)
    }
}
