//! An operation-based field's merge: each block's operation applies to the
//! document value exactly once. The applied key is the one-winner race that
//! makes a redelivered block a no-op.
use super::*;
use crdt::stored::StoredDocument;
use crdt::FieldCrdt;
use defra_core::block::FieldDelta;

use super::field::FieldMergeResult;

/// The counter after `op`, for a document stored on the active version and a
/// block from `delta`'s version: the stored document goes back to that
/// version, takes the operation there and migrates once. `None` when this
/// node knows no path between the two versions.
///
/// A lens that exports no `inverse` cannot take the document back, so its
/// operation is applied to the migrated counter as it is. That matches a
/// node still storing the block's version only when the lens shifts the
/// counter or leaves it alone; a lens that sets or scales it needs an inverse.
async fn counter_on_active_version(
    datastore: &NamespaceView,
    db: &crate::database::DB<impl Store>,
    collection: &Collection,
    field: &FieldDescription,
    delta: &FieldDelta<'_>,
    op: &document::FieldOp,
    doc_short_id: u64,
) -> std::result::Result<Option<NormalValue>, MergeError> {
    let systemstore = datastore.sibling(storage::namespace::Namespace::Systemstore);
    let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id)
        .await
        .map_err(MergeError::Database)?
    else {
        return Ok(None);
    };
    let doc_id =
        DocID::from_string(&doc_id).map_err(|error| MergeError::MergeFailed(error.to_string()))?;
    let Some(mut active) = collection
        .read_document_unvalidated(datastore, doc_short_id)
        .await
        .map_err(MergeError::Database)?
    else {
        return Ok(None);
    };
    active.set_id(doc_id);
    active.set_collection_version_id(collection.version_id());

    let versions = crate::definition::loader::get_collections_by_collection_id_known(
        &systemstore,
        collection.collection_id(),
        Some(collection),
    )
    .await
    .map_err(MergeError::Database)?;
    let Some(history) =
        crate::definition::lens::build_collection_history(&versions, collection.version_id())
    else {
        return Ok(None);
    };
    if !history.contains_key(delta.collection_version_id) {
        return Ok(None);
    }

    let failed = |error: lens::Error| MergeError::MergeFailed(error.to_string());
    let lens_document = crate::definition::lens::doc_to_lens_doc(&active)
        .ok_or_else(|| MergeError::MergeFailed("failed to decode a counter operand".into()))?;
    let source = match lens::transform_to_target(
        db.lens_store(),
        delta.collection_version_id,
        &history,
        lens::LensInput::new(collection.version_id(), lens_document),
    )
    .await
    {
        Ok(source) => source,
        Err(lens::Error::InverseNotExported) => {
            return counter_apply(collection, field, active.get(&field.name), op).map(Some);
        }
        Err(error) => return Err(failed(error)),
    };
    let mut source =
        crate::definition::migration::helpers::lens_doc_to_document(source, &active, collection);
    let updated = counter_apply(collection, field, source.get(&field.name), op)?;
    source.set_unchecked(field.name.to_owned(), updated);

    let lens_document = crate::definition::lens::doc_to_lens_doc(&source)
        .ok_or_else(|| MergeError::MergeFailed("failed to decode a merged counter".into()))?;
    let migrated = lens::transform_to_target(
        db.lens_store(),
        collection.version_id(),
        &history,
        lens::LensInput::new(delta.collection_version_id, lens_document),
    )
    .await
    .map_err(failed)?;
    let migrated =
        crate::definition::migration::helpers::lens_doc_to_document(migrated, &source, collection);
    Ok(migrated.get(&field.name).cloned())
}

fn counter_apply(
    collection: &Collection,
    field: &FieldDescription,
    current: Option<&NormalValue>,
    op: &document::FieldOp,
) -> std::result::Result<NormalValue, MergeError> {
    let crdt = collection
        .crdts()
        .field(field.crdt_type)
        .ok_or_else(|| MergeError::MergeFailed("counter field has no CRDT".into()))?;
    crdt.apply(field, current, op)
        .map_err(|error| MergeError::MergeFailed(error.to_string()))
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Apply a block's operation to `field` once. A `standalone` block finds
    /// no operand target when the document has no value on its schema
    /// version; it is then deferred untouched, applied key unclaimed, so its
    /// composite applies it.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_field_op_once(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        crdt: &dyn FieldCrdt,
        collection: &Collection,
        field: &FieldDescription,
        delta: &FieldDelta<'_>,
        doc_short_id: u64,
        standalone: bool,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let input = crdt
            .decode(field, delta.data)
            .map_err(|error| MergeError::BlockDecode(format!("field '{}': {error}", field.name)))?;
        let state = crdt.state(field, delta.priority, &input).map_err(failed)?;
        let op = crdt.op(input, false).map_err(failed)?;
        let off_active = delta.collection_version_id != collection.version_id();
        let stored_version = if off_active {
            collection
                .load_version(datastore, doc_short_id)
                .await
                .map_err(MergeError::Database)?
        } else {
            None
        };
        // Only a document stored on the active version needs the operation
        // carried there. One stored on the block's own version takes it as it
        // is, and a read migrates the whole document.
        let translate_counter =
            crdt.ctype().is_counter() && stored_version.as_deref() == Some(collection.version_id());
        let source = if stored_version.as_deref() == Some(delta.collection_version_id) {
            source_collection(datastore, collection, delta.collection_version_id).await?
        } else {
            None
        };
        let target = if translate_counter {
            None
        } else {
            op_target(
                datastore,
                source.as_ref().unwrap_or(collection),
                doc_short_id,
                field,
            )
            .await?
        };
        if target.is_none() && standalone {
            return Ok(FieldMergeResult {
                applied: false,
                value: None,
                order: None,
                deferred: true,
                op: None,
                parked: None,
                superseded: false,
                active_result: false,
            });
        }
        let applied_key =
            storage::keys::HeadstorePriorityKey::new(doc_short_id, delta.priority, *cid).bytes();
        let applied = !headstore.has(&applied_key).await.map_err(storage)?;

        let value = if translate_counter {
            let translated = if applied {
                counter_on_active_version(
                    datastore,
                    &self.db,
                    collection,
                    field,
                    delta,
                    &op,
                    doc_short_id,
                )
                .await?
            } else {
                None
            };
            match translated {
                Some(value) => Some(value),
                None => {
                    let stored = collection
                        .read_document_unvalidated(datastore, doc_short_id)
                        .await
                        .map_err(MergeError::Database)?;
                    let stored = stored.as_ref().and_then(|doc| doc.get(&field.name));
                    if applied {
                        Some(crdt.apply(field, stored, &op).map_err(failed)?)
                    } else {
                        stored.cloned()
                    }
                }
            }
        } else {
            match target {
                Some(part) => {
                    let doc_key = collection.doc_key(doc_short_id);
                    if applied {
                        let operand = storage::merge::document_part(part, state.part())
                            .ok_or_else(|| {
                                MergeError::MergeFailed(format!(
                                    "invalid operation on '{}'",
                                    field.name
                                ))
                            })?;
                        crate::database::stored_crdts::record(
                            datastore,
                            crate::database::stored_crdts::custom([state.kind]),
                        )
                        .await
                        .map_err(storage)?;
                        datastore.merge(&doc_key, &operand).await.map_err(storage)?;
                    }
                    // The read is never validated: no decision rests on it.
                    datastore
                        .get_parts(&doc_key, &[])
                        .await
                        .map_err(storage)?
                        .as_deref()
                        .and_then(StoredDocument::decode)
                        .and_then(|stored| stored.fields.get(&part)?.materialize(self.db.crdts()))
                }
                None => {
                    if !applied {
                        collection
                            .read_document_unvalidated(datastore, doc_short_id)
                            .await
                            .map_err(MergeError::Database)?
                            .as_ref()
                            .and_then(|doc| doc.get(&field.name))
                            .cloned()
                    } else {
                        let stored = collection
                            .read_document_unvalidated(datastore, doc_short_id)
                            .await
                            .map_err(MergeError::Database)?;
                        let stored = stored.as_ref().and_then(|doc| doc.get(&field.name));
                        Some(crdt.apply(field, stored, &op).map_err(failed)?)
                    }
                }
            }
        };
        if applied {
            headstore
                .set(&applied_key, crate::block::heads::document::APPLIED)
                .await
                .map_err(storage)?;
        }
        Ok(FieldMergeResult {
            applied,
            value,
            order: None,
            deferred: false,
            op: None,
            parked: None,
            superseded: false,
            active_result: translate_counter,
        })
    }

    pub(crate) async fn best_effort_finalize_linked_field_blocks(&self, cids: &[Cid]) {
        if cids.is_empty() {
            return;
        }
        // The blockstore's merged-set is the single source of truth for CRDT
        // idempotency (matching Go): every ingest path (PushLog, DAG traversal,
        // crash-recovery replay) skips blocks it holds. See #847.
        if let Err(error) = self.blockstore.mark_batch_as_merged(cids).await {
            tracing::warn!(
                count = cids.len(),
                error = %error,
                "Failed to finalize merged linked field blocks"
            );
        }
    }
}

/// `collection` at `version`, another version of it that this node knows.
async fn source_collection(
    datastore: &NamespaceView,
    collection: &Collection,
    version: &str,
) -> std::result::Result<Option<Collection>, MergeError> {
    let systemstore = datastore.sibling(storage::namespace::Namespace::Systemstore);
    Ok(
        crate::definition::loader::get_collection_by_version_id(&systemstore, version)
            .await
            .map_err(MergeError::Database)?
            .filter(|schema| {
                !schema.is_placeholder && schema.collection_id == collection.collection_id()
            })
            .map(|schema| Collection::new(schema, datastore.crdts().clone())),
    )
}

/// The field's part of the document value, as an operand target, when the
/// stored value is on `collection`'s schema version and has part IDs. `None`
/// otherwise: such a value is rewritten whole by its composite.
pub(super) async fn op_target(
    datastore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    field: &FieldDescription,
) -> std::result::Result<Option<u32>, MergeError> {
    let Some(part) = collection
        .part_ids(datastore)
        .await
        .map_err(MergeError::Database)?
        .and_then(|ids| ids.get(&field.name).copied())
    else {
        return Ok(None);
    };
    let version = collection
        .load_version(datastore, doc_short_id)
        .await
        .map_err(MergeError::Database)?;
    Ok((version.as_deref() == Some(collection.version_id())).then_some(part))
}
