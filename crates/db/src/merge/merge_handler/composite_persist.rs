use super::composite::{CompositeMergeContext, CompositeMergeState};
use super::*;

/// Marker byte indicating a document is deleted (matches Go's DeletedObjectMarker).
const DELETED_MARKER: u8 = 0x01;

/// Build the deletion marker key: /del/{collection_id}/{doc_short_id}
fn build_deleted_key(collection_id: &str, doc_short_id: u64) -> Vec<u8> {
    storage::keys::deleted_doc_key(collection_id, doc_short_id)
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(crate) async fn persist_merged_document(
        &self,
        datastore: &mut NamespaceView,
        systemstore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        state: &mut CompositeMergeState,
    ) -> std::result::Result<Option<MergeOutcome>, MergeError> {
        let collection = context.collection.as_ref().ok_or_else(|| {
            MergeError::MissingMetadata(format!(
                "Collection not found for collection_version_id: {}",
                context.payload.collection_version_id
            ))
        })?;

        collection
            .validate_write_definition(datastore, false)
            .await
            .map_err(MergeError::Database)?;
        if self
            .composite_merge_hook()
            .is_some_and(|hook| hook.supports_transactions())
        {
            if let Some(outcome) = self
                .check_protected_update(
                    context.cid,
                    context.block,
                    context.payload,
                    context.doc_id_str,
                    collection.schema(),
                    Some(systemstore.sibling(storage::namespace::Namespace::Acpstore)),
                )
                .await?
            {
                return Ok(Some(outcome));
            }
        }
        state.is_branchable = collection.schema().is_branchable;

        if context.payload.status == 2 {
            self.handle_deletion(datastore, context, collection).await?;
            return Ok(None);
        }

        if state.field_values.is_empty() && !state.parked_fields {
            return Ok(None);
        }

        let doc_id = DocID::from_string(context.doc_id_str)
            .map_err(|e| MergeError::MergeFailed(format!("Invalid doc_id: {}", e)))?;

        let (mut doc, old_doc) = match state
            .reads
            .document(collection, datastore, context.doc_short_id, &doc_id)
            .await
        {
            Ok(Some(existing)) => {
                let old = existing.clone();
                (existing, Some(old))
            }
            Err(error) => return Err(MergeError::Database(error)),
            Ok(None) => {
                let mut new_doc = Document::new(collection.schema_arc());
                new_doc.set_id(doc_id.clone());
                (new_doc, None)
            }
        };

        doc.set_collection_version_id(&context.payload.collection_version_id);

        for (field_name, value) in &state.field_values {
            doc.set_unchecked(field_name.clone(), value.clone());
        }
        for (field_name, op) in &state.field_ops {
            doc.edit(field_name.clone(), op.clone())
                .map_err(|e| MergeError::MergeFailed(e.to_string()))?;
        }

        let known_fields: rapidhash::RapidHashSet<&str> = collection
            .schema()
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let all_field_names: Vec<String> = doc.field_names().map(|name| name.to_string()).collect();
        for field_name in &all_field_names {
            if !known_fields.contains(field_name.as_str()) {
                doc.remove(field_name);
            }
        }

        // @immutable enforcement happens in process_linked_field_blocks (phase 1),
        // BEFORE any field block is persisted, so a rejected block leaves no
        // partial write.

        collection
            .save_with_datastore_priorities(
                datastore,
                &doc,
                context.doc_short_id,
                Some(&state.field_priorities),
            )
            .await
            .map_err(MergeError::Database)?;
        state.reads.wrote_document();

        // A merged write onto a logically-deleted document must not touch the
        // indexes: the document is dead, and indexing it re-mints exactly the
        // stale unique entry that blocks the value forever (source-inc/gents#700's
        // out-of-order create-after-delete arrival). Go skips index sync when
        // the merged doc reads back absent for the same reason.
        let deleted_marker_key =
            build_deleted_key(collection.collection_id(), context.doc_short_id);
        let doc_is_tombstoned = datastore
            .has(&deleted_marker_key)
            .await
            .map_err(|e| MergeError::Database(crate::error::Error::Storage(e)))?;

        // The retention rule judges the document as merged, alive ones only;
        // a refusal discards the attempt, so nothing of it is stored.
        if !doc_is_tombstoned {
            if let Some(refusal) = self
                .db
                .options()
                .retention_floor(collection.schema())
                .and_then(|floor| floor.refusal(&doc))
            {
                self.db.merge_stats().record_retention_refusal(refusal);
                return Err(MergeError::RetentionRefused(refusal));
            }
        }

        if old_doc.is_none() && !doc_is_tombstoned {
            if context.block.heads.as_deref().is_none_or(<[Cid]>::is_empty) {
                if let (Some(provider), Some(policy), Some(signer)) = (
                    self.db.document_acp(),
                    collection.schema().policy.as_ref(),
                    self.verify_block_signature(context.cid, context.block, &[])
                        .await?,
                ) {
                    if let Some(scoped) = provider.bind_transaction(
                        systemstore.sibling(storage::namespace::Namespace::Acpstore),
                    ) {
                        let signer = identity::Did::new(signer)
                            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
                        scoped
                            .register_doc_object(
                                &signer,
                                &policy.id,
                                &policy.resource_name,
                                context.doc_id_str,
                            )
                            .await
                            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
                    }
                }
            }

            crate::event::arrivals::record(
                systemstore,
                collection.resolved_root_id(),
                context.doc_id_str,
            )
            .await
            .map_err(MergeError::Database)?;
        }

        let short_id = collection.resolved_root_id();
        match IndexManager::from_collection(short_id, collection.schema()) {
            Ok(index_manager) if !doc_is_tombstoned => {
                // Merge-path index maintenance resolves live unique conflicts
                // deterministically (smallest public DocID wins) instead of
                // failing: a CRDT merge cannot preserve cross-replica
                // uniqueness, and failing here wedges the document's entire
                // forward history in permanent retry on both replicas (#1111).
                // Both documents persist; the index converges to the same
                // winner everywhere.
                let index_result = match &old_doc {
                    Some(old_doc) => {
                        index_manager
                            .on_document_update_merge(
                                datastore,
                                systemstore,
                                old_doc,
                                &doc,
                                context.doc_short_id,
                                collection.schema(),
                            )
                            .await
                    }
                    None => {
                        index_manager
                            .on_document_create_merge(
                                datastore,
                                systemstore,
                                &doc,
                                context.doc_short_id,
                                collection.schema(),
                            )
                            .await
                    }
                };
                let index_result = index_result.map(|conflicts| {
                    state.unique_conflicts.extend(conflicts);
                });
                if let Err(e) = index_result {
                    // The merge variants above already resolve the common live
                    // unique-conflict case deterministically without erroring.
                    // What still surfaces `UniqueConstraintViolation` here are
                    // the degenerate arms (non-Unique index type mismatch,
                    // empty collection_id, `conflicting_doc_id` returning
                    // None) — internal index-state inconsistency, not a
                    // transient storage failure. Classify and reject rather
                    // than retrying forever.
                    if e.is_unique_constraint_violation() {
                        return Err(MergeError::UniqueConstraintViolation(e.to_string()));
                    }
                    let message = if context.mode.is_standalone() {
                        "Failed to update indexes after merge"
                    } else {
                        "Failed to update indexes after batch merge"
                    };
                    return Err(MergeError::MergeFailed(format!("{message}: {e}")));
                }
            }
            Ok(_) => {
                tracing::info!(
                    doc_id = %context.doc_id_str,
                    collection = %collection.name(),
                    "skipping index maintenance for merge onto a tombstoned document"
                );
            }
            Err(e) => {
                // Previously swallowed silently, skipping ALL index maintenance
                // with no trace — enforcement disappeared invisibly (#1111).
                tracing::error!(
                    doc_id = %context.doc_id_str,
                    collection = %collection.name(),
                    error = %e,
                    "index manager could not be built from the collection schema; \
                     skipping index maintenance for this merge"
                );
            }
        }

        if let Some(enc_key) = self.se_enc_key() {
            if let Err(e) = se_merge::generate_merge_artifacts(
                datastore,
                collection.schema(),
                context.doc_id_str,
                &state.field_values,
                enc_key,
                None,
            )
            .await
            {
                let message = if context.mode.is_standalone() {
                    "Failed to generate SE artifacts after merge"
                } else {
                    "Failed to generate SE artifacts after batch merge"
                };
                tracing::warn!(
                    doc_id = %context.doc_id_str,
                    error = %e,
                    "{message}"
                );
            }
        }

        if context.mode.is_standalone() {
            tracing::info!(
                doc_id = %context.doc_id_str,
                collection = %collection.name(),
                fields_count = state.field_values.len(),
                any_applied = state.any_field_applied,
                "Document stored for queries"
            );
        }

        Ok(None)
    }

    pub(crate) async fn handle_deletion(
        &self,
        datastore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        collection: &Collection,
    ) -> std::result::Result<(), MergeError> {
        if let Ok(doc_id) = DocID::from_string(context.doc_id_str) {
            if let Ok(Some(old_doc)) = collection
                .get_with_datastore(datastore, context.doc_short_id, &doc_id)
                .await
            {
                collection
                    .release_series(datastore, &old_doc)
                    .await
                    .map_err(MergeError::Database)?;
                let short_id = collection.resolved_root_id();
                if let Ok(index_manager) =
                    IndexManager::from_collection(short_id, collection.schema())
                {
                    if let Err(e) = index_manager
                        .on_document_delete(
                            datastore,
                            &old_doc,
                            context.doc_short_id,
                            collection.schema(),
                        )
                        .await
                    {
                        let message = if context.mode.is_standalone() {
                            "Failed to delete indexes after merge"
                        } else {
                            "Failed to delete indexes after batch merge"
                        };
                        return Err(MergeError::MergeFailed(format!("{message}: {e}")));
                    }
                }
            }
        }

        if let (Some(provider), Some(policy)) =
            (self.db.document_acp(), collection.schema().policy.as_ref())
        {
            if let Some(scoped) = provider
                .bind_transaction(datastore.sibling(storage::namespace::Namespace::Acpstore))
            {
                scoped
                    .unregister_doc_object(&policy.id, &policy.resource_name, context.doc_id_str)
                    .await
                    .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
            }
        }
        let deleted_key = build_deleted_key(collection.collection_id(), context.doc_short_id);
        datastore
            .set(&deleted_key, &[DELETED_MARKER])
            .await
            .map_err(|e| MergeError::Database(crate::error::Error::Storage(e)))?;

        if context.mode.is_standalone() {
            tracing::info!(
                doc_id = %context.doc_id_str,
                collection = %collection.name(),
                "Deletion marker set after P2P merge"
            );
        }

        Ok(())
    }
}
