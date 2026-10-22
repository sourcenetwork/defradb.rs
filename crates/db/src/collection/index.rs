use super::*;

impl Collection {
    /// Create a new document blob and update all indexes.
    ///
    /// The document must already carry its (genesis-CID-derived) DocID and
    /// the caller supplies the allocated short ID. Duplicate detection
    /// happens against the DocID mapping before this is called, so no
    /// blob-level existence check remains.
    pub async fn create_with_indexes(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        index_manager: &IndexManager,
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

        // Store schema version for lens migration support
        self.store_version(datastore, doc_short_id).await?;

        // Update indexes
        index_manager
            .on_document_create(datastore, doc, doc_short_id, &self.def)
            .await?;

        Ok(doc_id)
    }

    /// Update an existing document and maintain all indexes.
    ///
    /// This method wraps the standard update operation with index maintenance.
    pub async fn update_with_indexes(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        index_manager: &IndexManager,
    ) -> Result<()> {
        self.update_with_indexes_parts(datastore, doc, doc_short_id, index_manager, None)
            .await
    }

    /// With `update`, a partial update of those fields, written as operands
    /// ordered by the document heads it builds its blocks from, by the run of
    /// its write that decides; without, a structural rewrite of the whole
    /// value.
    pub(crate) async fn update_with_indexes_parts(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
        doc_short_id: u64,
        index_manager: &IndexManager,
        update: Option<(
            &rapidhash::RapidHashSet<String>,
            &crate::block::builder::DocHeadsSnapshot,
            query::update_parts::Run,
        )>,
    ) -> Result<()> {
        let modified = update.map(|(fields, _, _)| fields);
        // Validate document against schema
        self.validate_document(doc)?;

        let doc_id = doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("Document must have an ID".into()))?;

        if self.is_deleted(datastore, doc_short_id).await? {
            return Err(Error::DocumentNotFound(doc_id.to_string()));
        }

        // Get old document for index update
        let old_doc = match update {
            Some((changed, _, run)) => {
                let fields = run.written(&self.def, index_manager.get_indexes(), changed, |name| {
                    doc.pending_op(name).is_some()
                });
                self.get_with_datastore_fields(datastore, doc_short_id, doc_id, &fields)
                    .await?
            }
            None => {
                self.get_with_datastore(datastore, doc_short_id, doc_id)
                    .await?
            }
        };
        let old_doc = match old_doc {
            Some(doc) => doc,
            None => return Err(Error::DocumentNotFound(doc_id.to_string())),
        };

        // Mutation callers may hold a document fetched before this
        // transaction began. For touched composite indexes, use fresh values
        // for their unmodified components while preserving the caller's patch
        // fields. The projected read above includes every such dependency.
        let mut indexed_new_doc = doc.clone();
        if let Some(changed) = modified {
            for index in index_manager.get_indexes() {
                if !index
                    .fields
                    .iter()
                    .any(|field| changed.contains(&field.name))
                {
                    continue;
                }
                for field in &index.fields {
                    if changed.contains(&field.name) {
                        continue;
                    }
                    indexed_new_doc.set_unchecked(
                        field.name.clone(),
                        old_doc
                            .get(&field.name)
                            .cloned()
                            .unwrap_or(NormalValue::Null),
                    );
                }
            }
        }

        match modified {
            Some(changed) => self.validate_modified_immutable_fields(&old_doc, doc, changed)?,
            None => self.validate_immutable_fields_unchanged(&old_doc, doc)?,
        }

        let series_changed =
            modified.is_none_or(|changed| changed.contains(crate::downsample::SERIES_FIELD));
        if series_changed {
            self.release_series(datastore, &old_doc).await?;
            self.claim_series(datastore, doc).await?;
        }

        match update {
            Some((fields, heads, _)) => {
                self.merge_local_document_value(datastore, doc_short_id, doc, fields, heads)
                    .await?
            }
            None => {
                self.put_document_value(datastore, doc_short_id, doc, false, None)
                    .await?
            }
        }

        // Update schema version to current collection version
        self.store_version(datastore, doc_short_id).await?;

        // Update indexes
        match modified {
            Some(changed) => {
                index_manager
                    .on_document_update_modified(
                        datastore,
                        &old_doc,
                        &indexed_new_doc,
                        doc_short_id,
                        &self.def,
                        changed,
                    )
                    .await?
            }
            None => {
                index_manager
                    .on_document_update(datastore, &old_doc, doc, doc_short_id, &self.def)
                    .await?
            }
        }

        Ok(())
    }

    /// Delete a document and update all indexes.
    ///
    /// This uses logical deletion: the document data remains in storage but a
    /// deletion marker is set. This allows `showDeleted: true` queries to still
    /// return the document with `_deleted: true`.
    pub async fn delete_with_indexes(
        &self,
        datastore: &NamespaceView,
        doc_id: &DocID,
        doc_short_id: u64,
        index_manager: &IndexManager,
    ) -> Result<bool> {
        self.validate_write_definition(datastore, true).await?;

        let key = self.doc_key(doc_short_id);
        let deleted_key = self.deleted_key(doc_short_id);

        // Check if already deleted
        if datastore.has(&deleted_key).await.map_err(Error::Storage)? {
            return Ok(false); // Already deleted
        }

        // Get the document for index cleanup
        let doc = match datastore.get(&key).await.map_err(Error::Storage)? {
            Some(bytes) => {
                let mut d = crdt::stored::document_from_storage(&bytes, self.schema_arc())?;
                d.set_id(doc_id.clone());
                d
            }
            None => return Ok(false),
        };

        self.release_series(datastore, &doc).await?;

        // Set deletion marker (logical delete, keep document data)
        datastore
            .set(&deleted_key, &[DELETED_MARKER])
            .await
            .map_err(Error::Storage)?;

        // Update indexes (remove from indexes since doc is now "deleted")
        index_manager
            .on_document_delete(datastore, &doc, doc_short_id, &self.def)
            .await?;

        Ok(true)
    }

    /// Check if a document is marked as deleted.
    pub async fn is_deleted(&self, datastore: &NamespaceView, doc_short_id: u64) -> Result<bool> {
        let deleted_key = self.deleted_key(doc_short_id);
        datastore.has(&deleted_key).await.map_err(Error::Storage)
    }
}
