use super::*;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    #[allow(clippy::too_many_arguments)]
    async fn seed_lww_from_existing_doc(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        payload: &defra_core::block::LwwDeltaPayload,
        fallback_collection_id: Option<&str>,
        lww: &Lww,
        doc_id_str: &str,
        doc_short_id: u64,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<bool, MergeError> {
        let Some(collection) = self
            .block_collection(&payload.collection_version_id, fallback_collection_id)
            .await?
        else {
            return Ok(false);
        };

        // Two priority stores can disagree for a field: the headstore tracks the
        // authoritative current priority (advanced by BOTH local writes and
        // merges), while the datastore LWW priority is advanced ONLY by merges.
        // A local write therefore pushes the headstore (and materialized doc)
        // ahead of a stale datastore LWW. Reconcile here: when the headstore is
        // ahead, re-seed the datastore LWW from the materialized doc value at the
        // headstore priority, so the merge resolves against the true current
        // state instead of the stale CRDT entry (otherwise a re-walked ancestor
        // delta clobbers a concurrent local write — replicas diverge).
        let hs_priority = reads
            .field_priority(headstore, doc_short_id, &payload.field_name)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;
        let stored_priority = crdt::traits::PriorityReader::priority(lww, datastore)
            .await
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        // Fast path: the datastore LWW is strictly ahead of the headstore, so
        // nothing in the materialized doc can improve it. Equal priority still
        // needs the value tie-break below to preserve Go LWW behavior.
        if stored_priority != 0 && stored_priority > hs_priority {
            return Ok(true);
        }

        let doc_id = match DocID::from_string(doc_id_str) {
            Ok(doc_id) => doc_id,
            Err(_) => return Ok(false),
        };

        let Some(existing_doc) = reads
            .document(&collection, datastore, doc_short_id, &doc_id)
            .await?
        else {
            return Ok(false);
        };

        let Some(field_value) = existing_doc.get(&payload.field_name) else {
            return Ok(true);
        };

        let priority = hs_priority;
        if priority == 0 {
            return Ok(true);
        }

        let mut value_bytes = Vec::new();
        ciborium::into_writer(field_value, &mut value_bytes)
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        let should_seed = match priority.cmp(&stored_priority) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => {
                match crdt::traits::ValueReader::value(lww, datastore).await {
                    Ok(stored_value) => value_bytes > stored_value,
                    Err(_) => true,
                }
            }
        };

        if !should_seed {
            return Ok(true);
        }

        let seed_delta = LwwDelta::new(
            doc_id_str.as_bytes().to_vec(),
            payload.field_name.clone(),
            priority,
            payload.collection_version_id.clone(),
            value_bytes,
        )
        .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        let seed_ctx = Context {
            doc_id: DocId::new(doc_id_str).map_err(|e| MergeError::MergeFailed(e.to_string()))?,
            schema_version: payload.collection_version_id.clone(),
            is_create: false,
        };

        lww.merge(datastore, &seed_ctx, &seed_delta)
            .await
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        Ok(true)
    }

    /// Process an LWW delta within an existing transaction, returning the merge result
    /// and the winning value for document reconstruction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_lww_delta_in_txn(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        payload: &defra_core::block::LwwDeltaPayload,
        fallback_collection_id: Option<&str>,
        doc_id_str: &str,
        doc_short_id: u64,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<LwwMergeResult, MergeError> {
        tracing::debug!(
            cid = %cid,
            field_name = %payload.field_name,
            priority = payload.priority,
            "Processing LWW delta in transaction"
        );

        // Create the LWW CRDT for this field
        let lww = Lww::new(
            payload.collection_version_id.clone(),
            doc_id_str.as_bytes(),
            payload.field_name.clone(),
        )
        .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        // Create the delta
        let delta = LwwDelta::new(
            doc_id_str.as_bytes().to_vec(),
            payload.field_name.clone(),
            payload.priority,
            payload.collection_version_id.clone(),
            payload.data.clone(),
        )
        .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        let doc_exists = self
            .seed_lww_from_existing_doc(
                datastore,
                headstore,
                payload,
                fallback_collection_id,
                &lww,
                doc_id_str,
                doc_short_id,
                reads,
            )
            .await?;
        let ctx = Context {
            doc_id: DocId::new(doc_id_str).map_err(|e| MergeError::MergeFailed(e.to_string()))?,
            schema_version: payload.collection_version_id.clone(),
            is_create: payload.priority == 1 && !doc_exists,
        };

        // Perform the merge
        let merge_result = lww
            .merge(datastore, &ctx, &delta)
            .await
            .map_err(|e| MergeError::MergeFailed(e.to_string()))?;

        let applied = merge_result.was_applied();
        let raw_value = lww
            .value_or_tombstone(datastore)
            .await
            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
        let value = Some(if raw_value.is_empty() {
            NormalValue::Null
        } else {
            ciborium::from_reader::<NormalValue, _>(raw_value.as_ref())
                .map_err(|error| MergeError::BlockDecode(error.to_string()))?
        });

        let priority = crdt::traits::PriorityReader::priority(&lww, datastore)
            .await
            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
        Ok(LwwMergeResult {
            applied,
            value,
            priority,
            raw_value: raw_value.to_vec(),
        })
    }
}
