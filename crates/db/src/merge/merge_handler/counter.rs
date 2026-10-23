use super::*;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Whether `cid` was already merged. A store that cannot say fails the
    /// merge as retryable: reading it as "not merged" would apply the block
    /// again.
    async fn block_is_merged(&self, cid: &Cid) -> std::result::Result<bool, MergeError> {
        self.blockstore
            .is_merged(cid)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))
    }

    /// Process a Counter delta from a block (standalone, with its own transaction).
    pub async fn process_counter_delta(
        &self,
        cid: &Cid,
        payload: &defra_core::block::CounterDeltaPayload,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<MergeOutcome, MergeError> {
        let Some(doc_id_str) = self.resolve_field_block_doc_id(cid).await? else {
            return Ok(MergeOutcome::terminal_skip(
                "field block has no unambiguous owner; merged via its composite",
            ));
        };

        // A replay of a finalized block ends here, before it needs the
        // collection; the check after the queue covers a concurrent delivery.
        if self.block_is_merged(cid).await? {
            return Ok(MergeOutcome::terminal_skip("already merged"));
        }

        // Look up the collection to determine field kind and counter type,
        // with fallback to metadata's collection_id for cross-version sync
        let missing_collection = || {
            MergeError::MissingMetadata(format!(
                "Collection not found for collection_version_id: {}",
                payload.collection_version_id
            ))
        };
        let collection = self
            .block_collection(&payload.collection_version_id, metadata.collection_id)
            .await?
            .ok_or_else(missing_collection)?;

        tracing::debug!(
            cid = %cid,
            field_name = %payload.field_name,
            doc_id = %doc_id_str,
            priority = payload.priority,
            nonce = payload.nonce,
            "Processing Counter delta"
        );

        if self.block_is_merged(cid).await? {
            tracing::debug!(
                cid = %cid,
                field_name = %payload.field_name,
                "Counter delta already marked merged, skipping standalone replay"
            );
            return Ok(MergeOutcome::terminal_skip("already merged"));
        }

        // Get field definition to determine numeric kind and allow_decrement
        let field = collection
            .schema()
            .field_by_name(&payload.field_name)
            .ok_or_else(|| {
                MergeError::MissingMetadata(format!(
                    "Field '{}' not found in collection",
                    payload.field_name
                ))
            })?;

        // Determine numeric kind from field type
        let numeric_kind = self.get_numeric_kind_from_field(field)?;

        let delta = self.create_counter_delta_for(payload, numeric_kind, &doc_id_str)?;

        // Create a new transaction for this merge
        let txn = self.db.new_txn(false).await?;
        txn.validate_collection_schema_for_write(&collection)
            .await
            .map_err(MergeError::Database)?;
        let doc_short_id = crate::docid::map::get_doc_ref(&txn.systemstore()?, &doc_id_str)
            .await
            .map_err(MergeError::Database)?
            .ok_or_else(|| {
                MergeError::MissingMetadata(format!("document identity not found for {doc_id_str}"))
            })?
            .doc_short_id;

        let result = {
            let mut datastore = txn.datastore()?;
            let headstore = txn.headstore()?;
            match counter_in_document(&datastore, &collection, doc_short_id, field, numeric_kind)
                .await
            {
                Ok(Some(counter)) => self
                    .merge_counter_once(
                        &mut datastore,
                        &headstore,
                        cid,
                        payload,
                        doc_short_id,
                        &counter,
                        &delta,
                    )
                    .await
                    .map(Some),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            }
        };

        match result {
            Ok(Some(result)) => {
                txn.force_commit().await?;
                self.best_effort_finalize_field_block_merge(cid).await;
                tracing::info!(
                    cid = %cid,
                    field_name = %payload.field_name,
                    doc_id = %doc_id_str,
                    "Counter delta merged successfully"
                );
                if result.applied {
                    Ok(MergeOutcome::Merged)
                } else {
                    Ok(MergeOutcome::terminal_skip("already merged"))
                }
            }
            Ok(None) => {
                txn.force_discard().map_err(MergeError::Database)?;
                // No document value on this block's schema version (the
                // document is not materialized yet, or its value is on another
                // version or predates part IDs), so the increment cannot be an
                // operand on it. The composite that links this block writes the
                // value and applies the increment there, once.
                Ok(MergeOutcome::terminal_skip(
                    "counter merged via its composite: no document value on its schema version",
                ))
            }
            Err(e) => {
                if let Err(discard_err) = txn.force_discard() {
                    tracing::error!(
                        cid = %cid,
                        discard_error = %discard_err,
                        merge_error = %e,
                        "Failed to discard transaction after merge error"
                    );
                }
                Err(MergeError::MergeFailed(e.to_string()))
            }
        }
    }

    /// Process a Counter delta within an existing transaction, returning the merge result
    /// and the accumulated value for document reconstruction.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_counter_delta_in_txn(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        payload: &defra_core::block::CounterDeltaPayload,
        fallback_collection_id: Option<&str>,
        doc_id_str: &str,
        doc_short_id: u64,
    ) -> std::result::Result<CounterMergeResult, MergeError> {
        tracing::debug!(
            cid = %cid,
            field_name = %payload.field_name,
            priority = payload.priority,
            nonce = payload.nonce,
            "Processing Counter delta in transaction"
        );

        // Look up the collection to determine field kind and counter type,
        // with fallback to metadata's collection_id for cross-version sync
        let collection = self
            .block_collection(&payload.collection_version_id, fallback_collection_id)
            .await?
            .ok_or_else(|| {
                MergeError::MissingMetadata(format!(
                    "Collection not found for collection_version_id: {}",
                    payload.collection_version_id
                ))
            })?;

        // Get field definition
        let field = collection
            .schema()
            .field_by_name(&payload.field_name)
            .ok_or_else(|| {
                MergeError::MissingMetadata(format!(
                    "Field '{}' not found in collection",
                    payload.field_name
                ))
            })?;

        let numeric_kind = self.get_numeric_kind_from_field(field)?;
        let delta = self.create_counter_delta_for(payload, numeric_kind, doc_id_str)?;
        if let Some(counter) =
            counter_in_document(datastore, &collection, doc_short_id, field, numeric_kind).await?
        {
            return self
                .merge_counter_once(
                    datastore,
                    headstore,
                    cid,
                    payload,
                    doc_short_id,
                    &counter,
                    &delta,
                )
                .await;
        }

        // The composite rewrites a value that is on another schema version (or
        // predates part IDs) whole, so the increment is added to the stored
        // value here and the composite puts the sum.
        let applied_key =
            storage::keys::HeadstorePriorityKey::new(doc_short_id, payload.priority, *cid);
        let stored = collection
            .read_document_unvalidated(datastore, doc_short_id)
            .await
            .map_err(MergeError::Database)?;
        let stored = stored.as_ref().and_then(|doc| doc.get(&payload.field_name));
        let kind = crate::collection::value::counter_kind(field).ok_or_else(|| {
            MergeError::UnsupportedDelta(format!(
                "Counter field '{}' has no counter kind",
                field.name
            ))
        })?;
        if headstore
            .has(&applied_key.bytes())
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
        {
            return Ok(CounterMergeResult {
                applied: false,
                value: stored.cloned(),
            });
        }
        let increment = delta
            .checked_value(numeric_kind, field.crdt_type.allows_decrement())
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;
        let value = document::stored::add_counter(&field.name, kind, stored, &increment)
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;
        headstore
            .set(&applied_key.bytes(), crate::block::heads::document::APPLIED)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;
        Ok(CounterMergeResult {
            applied: true,
            value: Some(value),
        })
    }

    /// Apply the delta once per block: the applied key is the one-winner race
    /// that makes a redelivered block a no-op.
    #[allow(clippy::too_many_arguments)]
    async fn merge_counter_once(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        payload: &defra_core::block::CounterDeltaPayload,
        doc_short_id: u64,
        counter: &Counter,
        delta: &CounterDelta,
    ) -> std::result::Result<CounterMergeResult, MergeError> {
        let applied_key =
            storage::keys::HeadstorePriorityKey::new(doc_short_id, payload.priority, *cid);
        let applied = !headstore
            .has(&applied_key.bytes())
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;
        if applied {
            let context = Context {
                doc_id: DocId::new(String::from_utf8_lossy(delta.doc_id()).into_owned())
                    .map_err(|e| MergeError::MergeFailed(e.to_string()))?,
                schema_version: payload.collection_version_id.clone(),
                is_create: false,
            };
            counter
                .merge(datastore, &context, delta)
                .await
                .map_err(|e| MergeError::MergeFailed(e.to_string()))?;
            headstore
                .set(&applied_key.bytes(), crate::block::heads::document::APPLIED)
                .await
                .map_err(|e| MergeError::Storage(e.to_string()))?;
        }
        let value = match ValueReader::value(counter, datastore).await {
            Ok(bytes) => decode_counter_value(&bytes, delta.kind()),
            Err(error) => {
                tracing::debug!(field_name = %payload.field_name, %error, "Could not read counter value");
                None
            }
        };
        Ok(CounterMergeResult { applied, value })
    }

    pub(crate) async fn best_effort_finalize_field_block_merge(&self, cid: &Cid) {
        if let Err(error) = mark_field_blocks_merged(&self.blockstore, &[*cid]).await {
            tracing::warn!(
                cid = %cid,
                error = %error,
                "Failed to finalize merged field block"
            );
        }
    }

    pub(crate) async fn best_effort_finalize_linked_field_blocks(&self, cids: &[Cid]) {
        if cids.is_empty() {
            return;
        }

        if let Err(error) = mark_field_blocks_merged(&self.blockstore, cids).await {
            tracing::warn!(
                count = cids.len(),
                error = %error,
                "Failed to finalize merged linked field blocks"
            );
        }
    }
    /// Determine numeric kind from field definition
    fn get_numeric_kind_from_field(
        &self,
        field: &schema::FieldDescription,
    ) -> std::result::Result<NumericKind, MergeError> {
        use schema::FieldKind;

        match &field.kind {
            FieldKind::Scalar(scalar_kind) => {
                use schema::ScalarKind;
                match scalar_kind.base_kind() {
                    ScalarKind::Int => Ok(NumericKind::Int64),
                    ScalarKind::Float32 => Ok(NumericKind::Float32),
                    ScalarKind::Float64 => Ok(NumericKind::Float64),
                    other => Err(MergeError::UnsupportedDelta(format!(
                        "Counter field '{}' has unsupported scalar kind: {:?}",
                        field.name, other
                    ))),
                }
            }
            other => Err(MergeError::UnsupportedDelta(format!(
                "Counter field '{}' has unsupported kind: {:?}",
                field.name, other
            ))),
        }
    }

    /// Create a CounterDelta from the block payload
    fn create_counter_delta_for(
        &self,
        payload: &defra_core::block::CounterDeltaPayload,
        kind: NumericKind,
        doc_id_str: &str,
    ) -> std::result::Result<CounterDelta, MergeError> {
        // Go encodes counter data as CBOR. We need to decode it first.
        // The payload.data contains a CBOR-encoded i64, f32, or f64.
        match kind {
            NumericKind::Int64 => {
                let increment: i64 = ciborium::from_reader(&payload.data[..]).map_err(|e| {
                    MergeError::BlockDecode(format!(
                        "Failed to decode Counter Int64 increment: {}",
                        e
                    ))
                })?;
                CounterDelta::new_int64(
                    doc_id_str.as_bytes().to_vec(),
                    payload.field_name.clone(),
                    payload.priority,
                    payload.nonce,
                    payload.collection_version_id.clone(),
                    increment,
                )
                .map_err(|e| MergeError::MergeFailed(e.to_string()))
            }
            NumericKind::Float64 => {
                let increment: f64 = ciborium::from_reader(&payload.data[..]).map_err(|e| {
                    MergeError::BlockDecode(format!(
                        "Failed to decode Counter Float64 increment: {}",
                        e
                    ))
                })?;
                CounterDelta::new_float64(
                    doc_id_str.as_bytes().to_vec(),
                    payload.field_name.clone(),
                    payload.priority,
                    payload.nonce,
                    payload.collection_version_id.clone(),
                    increment,
                )
                .map_err(|e| MergeError::MergeFailed(e.to_string()))
            }
            NumericKind::Float32 => {
                let increment: f32 = ciborium::from_reader(&payload.data[..]).map_err(|e| {
                    MergeError::BlockDecode(format!(
                        "Failed to decode Counter Float32 increment: {}",
                        e
                    ))
                })?;
                CounterDelta::new_float32(
                    doc_id_str.as_bytes().to_vec(),
                    payload.field_name.clone(),
                    payload.priority,
                    payload.nonce,
                    payload.collection_version_id.clone(),
                    increment,
                )
                .map_err(|e| MergeError::MergeFailed(e.to_string()))
            }
            other => Err(MergeError::UnsupportedDelta(format!(
                "unsupported NumericKind {:?} for counter delta",
                other
            ))),
        }
    }
}

/// The counter's part of the document value, as an operand target, when the
/// stored value is on `collection`'s schema version and has part IDs. `None`
/// otherwise: such a value is rewritten whole by its composite.
async fn counter_in_document(
    datastore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    field: &schema::FieldDescription,
    numeric_kind: NumericKind,
) -> std::result::Result<Option<Counter>, MergeError> {
    let Some(part) = collection
        .part_ids(datastore)
        .await
        .map_err(MergeError::Database)?
        .and_then(|ids| ids.get(&field.name).copied())
    else {
        return Ok(None);
    };
    if collection
        .load_version(datastore, doc_short_id)
        .await
        .map_err(MergeError::Database)?
        .as_deref()
        != Some(collection.version_id())
    {
        return Ok(None);
    }
    Counter::new(
        collection.version_id().to_string(),
        collection.doc_key(doc_short_id),
        part,
        field.name.clone(),
        field.crdt_type.allows_decrement(),
        numeric_kind,
    )
    .map(Some)
    .map_err(|e| MergeError::MergeFailed(e.to_string()))
}

/// A counter part's stored bytes as a value of `kind`.
fn decode_counter_value(bytes: &[u8], kind: NumericKind) -> Option<NormalValue> {
    match kind {
        NumericKind::Int64 => Some(NormalValue::Int(i64::from_be_bytes(bytes.try_into().ok()?))),
        NumericKind::Float32 => Some(NormalValue::Float32(f32::from_be_bytes(
            bytes.try_into().ok()?,
        ))),
        NumericKind::Float64 => Some(NormalValue::Float64(f64::from_be_bytes(
            bytes.try_into().ok()?,
        ))),
        _ => None,
    }
}

/// Mark a batch of field-block CIDs as merged in the blockstore.
///
/// The blockstore's merged-set is the single source of truth for CRDT
/// idempotency in Rust (matching Go). Every ingest path upstream — PushLog,
/// DAG traversal, crash-recovery replay — gates on `is_merged(cid)` /
/// `get_unmerged()` and skips blocks that are already merged. See #847 for
/// the history of this contract.
async fn mark_field_blocks_merged<B: blockstore::Blockstore>(
    blockstore: &Arc<B>,
    cids: &[Cid],
) -> Result<(), MergeError> {
    if cids.is_empty() {
        return Ok(());
    }

    blockstore
        .mark_batch_as_merged(cids)
        .await
        .map_err(|e| MergeError::Storage(e.to_string()))?;

    Ok(())
}
