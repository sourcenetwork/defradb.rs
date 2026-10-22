use super::composite::{CompositeMergeContext, CompositeMergeState};
use super::*;

/// The delta is boxed: a `CrdtDelta` carries a collection definition payload,
/// which dwarfs the other two variants.
enum EffectiveLinkedDelta {
    Delta(Box<CrdtDelta>),
    Skip(MergeOutcome),
    SkipField,
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(crate) async fn process_linked_field_blocks(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        state: &mut CompositeMergeState,
    ) -> std::result::Result<Option<MergeOutcome>, MergeError> {
        let Some(links) = &context.block.links else {
            return Ok(None);
        };

        if context.mode.is_standalone() {
            tracing::info!(
                cid = %context.cid,
                links_count = links.len(),
                "Processing linked blocks from Composite delta"
            );
        }

        // Load the prior value once (deleted-inclusive, so a delete+recreate cannot
        // change an immutable field) when the composite links any @immutable field.
        let immutable_fields: RapidHashSet<&str> = context
            .collection
            .as_ref()
            .map(|collection| {
                collection
                    .schema()
                    .fields
                    .iter()
                    // JSON fields encode numbers differently in the document store
                    // (json_to_cbor_value) than in the field block (ciborium), so a
                    // cross-encoding merge comparison would false-reject. The local
                    // write path compares same-representation values and still
                    // enforces immutability for JSON fields.
                    .filter(|field| {
                        field.immutable
                            && !field
                                .kind
                                .as_scalar()
                                .is_some_and(|kind| kind.base_kind() == ScalarKind::NillableJson)
                    })
                    .map(|field| field.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let immutable_baseline = if context.collection.as_ref().is_some_and(|_| {
            links.iter().any(|l| {
                l.name == IMMUTABLE_LINK_NAME || immutable_fields.contains(l.name.as_str())
            })
        }) {
            let doc_id = DocID::from_string(context.doc_id_str)
                .map_err(|e| MergeError::MergeFailed(format!("invalid doc_id: {e}")))?;
            context
                .collection
                .as_ref()
                .unwrap()
                .get_with_datastore_include_deleted(datastore, context.doc_short_id, &doc_id, false)
                .await
                .map_err(MergeError::Database)?
                .map(|(doc, _)| doc)
        } else {
            None
        };

        // Phase 1: decode/decrypt every linked field ONCE and validate @immutable
        // BEFORE persisting anything, so a rejected composite leaves no partial write.
        // One entry per link: an Immutable block carries several field deltas.
        let mut pending: Vec<(Cid, Vec<CrdtDelta>)> = Vec::with_capacity(links.len());
        for dag_link in links {
            let link_name = &dag_link.name;
            let link_cid = &dag_link.link;

            let linked_block_data = match self.blockstore.get(link_cid).await {
                Ok(Some(data)) => data,
                Ok(None) => {
                    return Err(MergeError::Storage(format!(
                        "Linked block {} not found in blockstore",
                        link_cid
                    )));
                }
                Err(e) => return Err(MergeError::Storage(e.to_string())),
            };
            let linked_block = Block::from_dag_cbor(&linked_block_data)
                .map_err(|e| MergeError::BlockDecode(e.to_string()))?;
            state.owned_field_cids.push(*link_cid);
            if let Some(encryption_cid) = &linked_block.encryption {
                state.linked_encryption_cids.push(*encryption_cid);
            }

            if let Some(heads) = &linked_block.heads {
                state
                    .field_block_heads
                    .insert(link_name.clone(), heads.clone());
            }

            let effective = match self
                .handle_encryption(context, &linked_block, &mut state.encrypted_policy_checked)
                .await?
            {
                EffectiveLinkedDelta::Delta(delta) => *delta,
                EffectiveLinkedDelta::Skip(outcome) => return Ok(Some(outcome)),
                EffectiveLinkedDelta::SkipField => continue,
            };

            // An Immutable block applies as the LWW field deltas it replaces.
            let deltas = match effective {
                // Accepted whatever the local schema says: an ungoverned
                // collection's `@immutable` flags are not part of its synced
                // definition, so a peer may hold the fields as mutable.
                CrdtDelta::Immutable(payload) => {
                    let is_genesis = context.block.heads.as_ref().is_none_or(Vec::is_empty)
                        && context.payload.priority == 1;
                    // Under doc-level encryption every field is encrypted, so
                    // the builders write no Immutable block for that genesis.
                    let encrypted =
                        linked_block.encryption.is_some() || context.block.encryption.is_some();
                    if !is_genesis || payload.priority != 1 || encrypted {
                        return Err(MergeError::ImmutableFieldChanged(
                            "an immutable block is valid only unencrypted on a genesis composite"
                                .to_string(),
                        ));
                    }
                    state
                        .immutable_block_fields
                        .insert(*link_cid, payload.fields.keys().cloned().collect());
                    payload.lww_deltas().map(CrdtDelta::Lww).collect()
                }
                other => vec![other],
            };
            for effective in &deltas {
                let field_name = effective
                    .field()
                    .map_or(link_name.as_str(), |delta| delta.field_name);
                if immutable_fields.contains(field_name) {
                    if let Some(baseline) = immutable_baseline.as_ref() {
                        Self::check_immutable_delta(baseline, field_name, effective)?;
                    }
                }
            }
            pending.push((*link_cid, deltas));
        }

        // Phase 2: persist the decoded deltas.
        for (link_cid, deltas) in pending {
            for effective in deltas {
                let Some(delta) = effective.field() else {
                    return Err(MergeError::UnsupportedDelta(format!(
                        "Unexpected delta type in linked block: {:?}",
                        std::mem::discriminant(&effective)
                    )));
                };
                let field_name = delta.field_name.to_owned();
                state
                    .field_block_priorities
                    .insert(link_cid, delta.priority);
                let result = self
                    .merge_field_delta_in_txn(
                        datastore,
                        headstore,
                        &link_cid,
                        &effective,
                        context.metadata.collection_id,
                        context.doc_id_str,
                        context.doc_short_id,
                        &mut state.reads,
                        false,
                    )
                    .await?;
                match result.order {
                    Some(order) => {
                        state.field_priorities.insert(field_name.clone(), order);
                    }
                    // An operation is an operand on the stored value.
                    None => state.reads.wrote_document(),
                }
                if result.applied {
                    state.any_field_applied = true;
                }
                if let Some(value) = result.value {
                    state.field_values.insert(field_name, value);
                }
            }
            state.linked_field_cids.push(link_cid);
        }

        Ok(None)
    }

    /// Reject an incoming immutable-field delta that does not exactly preserve a
    /// prior value — a different value, or a tombstone/clear (empty payload) — so a
    /// crafted block cannot diverge the field store from the document store.
    ///
    /// A field with no prior value is allowed to be set: out-of-order sync can
    /// materialize a partial document before the create-composite carrying the
    /// immutable field merges, so rejecting first-set would false-reject the
    /// legitimate block.
    fn check_immutable_delta(
        baseline: &Document,
        field_name: &str,
        effective: &CrdtDelta,
    ) -> std::result::Result<(), MergeError> {
        let Some(prior) = baseline.get(field_name) else {
            return Ok(());
        };
        // An operation never restates a value, so only a state can be checked.
        let Some(delta) = effective.field() else {
            return Ok(());
        };
        if crdt::delta_crdt(effective, CType::None)
            .ok()
            .is_none_or(|crdt| crdt.replicates_ops())
        {
            return Ok(());
        }
        let data = delta.data;
        let preserves_prior = !data.is_empty()
            && ciborium::from_reader::<NormalValue, _>(data)
                .map(|incoming| incoming == *prior)
                .unwrap_or(false);
        if !preserves_prior {
            return Err(MergeError::ImmutableFieldChanged(format!(
                "immutable field '{field_name}' cannot be changed"
            )));
        }
        Ok(())
    }

    async fn handle_encryption(
        &self,
        context: &CompositeMergeContext<'_, '_>,
        linked_block: &Block,
        encrypted_policy_checked: &mut bool,
    ) -> std::result::Result<EffectiveLinkedDelta, MergeError> {
        if linked_block.encryption.is_some() && !*encrypted_policy_checked {
            *encrypted_policy_checked = true;
            if let (Some(collection), Some(hook)) = (
                context
                    .collection
                    .as_ref()
                    .filter(|collection| !self.is_governed(collection.schema())),
                self.composite_merge_hook(),
            ) {
                if let Some(outcome) = hook
                    .on_encrypted_link(context.doc_id_str, collection.schema(), context.metadata)
                    .await?
                {
                    return Ok(EffectiveLinkedDelta::Skip(outcome));
                }
            }
        }

        let mut effective_linked_delta = linked_block.delta.clone();
        if linked_block.encryption.is_some() {
            if let Some(data) = effective_linked_delta.field_data_mut() {
                match self
                    .decrypt_block_data(
                        data,
                        linked_block.encryption.as_ref(),
                        Some(context.metadata),
                    )
                    .await
                {
                    Ok(decrypted) => *data = decrypted,
                    Err(error @ (MergeError::NeedsDecryptionKey(_) | MergeError::Kms(_)))
                        if !matches!(error, MergeError::Kms(kms::Error::AccessDenied { .. })) =>
                    {
                        return Err(error)
                    }
                    Err(MergeError::Storage(_))
                        if matches!(
                            context.mode,
                            super::composite::CompositeMergeMode::History
                        ) =>
                    {
                        return Ok(EffectiveLinkedDelta::Skip(MergeOutcome::retryable_skip(
                            "history encrypted fields are not yet readable",
                        )));
                    }
                    Err(error) => {
                        tracing::debug!(
                            error = %error,
                            "Cannot decrypt field block, skipping field (canRead=false)"
                        );
                        return Ok(EffectiveLinkedDelta::SkipField);
                    }
                }
            }
        }

        Ok(EffectiveLinkedDelta::Delta(Box::new(
            effective_linked_delta,
        )))
    }
}
