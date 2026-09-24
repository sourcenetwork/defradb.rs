use super::*;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Process a CollectionDefinition delta - register synced collection schema in systemstore.
    ///
    /// When a peer receives collection definition blocks via Bitswap sync, this method
    /// reconstructs the `CollectionVersion` from the definition deltas and stores it
    /// in systemstore so `set_active_collection_version` can find and activate it.
    pub async fn process_collection_definition_delta(
        &self,
        cid: &Cid,
        block: &Block,
        payload: &CollectionDefinitionDeltaPayload,
        _metadata: &BlockMetadata<'_>,
    ) -> Result<MergeOutcome, MergeError> {
        // The version_id is the CID of this collection definition block
        let version_id = cid.to_string();

        // For patched versions, payload.name is None (name didn't change).
        // Resolve name and collection_id from the previous version via block.heads.
        let (collection_name, collection_id, prev_fields, previous) = match &payload.name {
            Some(name) => {
                // Initial version: the name is explicit and the collection ID
                // is this block's own CID, except that a policy reaches the
                // version and not the collection, so a policied block is named
                // by its version ID alone and the collection ID has to be
                // derived from the same block without it.
                (name.clone(), collection_id_of(cid, block)?, Vec::new(), None)
            }
            None => {
                // Patched version: look up previous version from heads
                let prev_version = self.resolve_previous_collection_version(block).await?;
                match prev_version {
                    Some(prev) => {
                        let name = prev.name.clone();
                        let col_id = prev.collection_id.clone();
                        let fields = prev.fields.clone();
                        (name, col_id, fields, Some(prev))
                    }
                    None => {
                        tracing::debug!(cid = %cid, "CollectionDefinition has no name and no resolvable previous version - skipping");
                        return Ok(MergeOutcome::terminal_skip(
                            "collection definition has no name and no previous version",
                        ));
                    }
                }
            }
        };

        tracing::info!(
            cid = %cid,
            collection_name = %collection_name,
            version_id = %version_id,
            "Processing collection definition delta"
        );

        // Load and decode linked field definition blocks (new fields for this version)
        let mut new_fields = Vec::new();
        if let Some(links) = &block.links {
            for link in links.iter() {
                let field_cid = &link.link;
                let field_bytes = self
                    .blockstore
                    .get(field_cid)
                    .await
                    .map_err(|e| MergeError::Storage(format!("Failed to load field block: {}", e)))?
                    .ok_or_else(|| {
                        MergeError::Storage(format!("Field block not found: {}", field_cid))
                    })?;

                let field_block = Block::from_dag_cbor(&field_bytes).map_err(|e| {
                    MergeError::BlockDecode(format!("Failed to decode field block: {}", e))
                })?;

                if let CrdtDelta::FieldDefinition(field_payload) = &field_block.delta {
                    let field_desc = self
                        .field_definition_to_description(field_payload, &field_cid.to_string())?;
                    new_fields.push(field_desc);
                } else {
                    tracing::warn!(
                        field_cid = %field_cid,
                        "Linked block is not a FieldDefinition - skipping"
                    );
                }
            }
        }

        // Merge previous version's fields with new fields from this delta.
        // For initial versions, prev_fields is empty so fields = new_fields.
        // For patched versions, combine existing fields + newly added fields.
        let mut fields = prev_fields;
        let existing_names: RapidHashSet<String> = fields.iter().map(|f| f.name.clone()).collect();
        for field in new_fields {
            if !existing_names.contains(&field.name) {
                fields.push(field);
            }
        }

        // Ensure _docID is first in the fields list (Go expects this ordering)
        if let Some(docid_pos) = fields.iter().position(|f| f.name == "_docID") {
            if docid_pos > 0 {
                let docid_field = fields.remove(docid_pos);
                fields.insert(0, docid_field);
            }
        }

        // Build the CollectionVersion by overlaying the delta onto the version
        // it supersedes, rather than by rebuilding from the delta alone.
        //
        // A delta carries a name, and per field a name, kind and CRDT type. It
        // carries none of the rest — the policy, the indexes in all four of
        // their flavours, the embeddings, the downsample configuration, the
        // collection set, whether the history is branchable or the collection
        // embedded-only. Listing what to rescue gets one more entry wrong every
        // time `CollectionVersion` grows a field, so start from everything the
        // previous version held and overlay only what this block actually says.
        //
        // Synced versions arrive inactive; a user activates one explicitly.
        let mut schema = match &previous {
            Some(previous) => {
                let mut schema = previous.clone();
                schema.name.clone_from(&collection_name);
                schema.version_id.clone_from(&version_id);
                schema.collection_id.clone_from(&collection_id);
                schema.fields = fields;
                schema
            }
            None => CollectionVersion::new(&collection_name, &version_id, &collection_id, fields),
        };
        schema.is_active = false;
        // This block is a definition, whatever stood in for it before.
        schema.is_placeholder = false;
        // Both are in the delta and in the identity, so a record rebuilt from
        // one that dropped them would describe a different collection from the
        // one the block names. Being in the identity is also why a patch never
        // restates them: changing either would change the collection ID, so a
        // patch inherits them from the version it supersedes like everything
        // else the overlay carries.
        if previous.is_none() {
            schema.is_branchable = payload.is_branchable;
            schema.governance_root.clone_from(&payload.governance_root);
        }

        // For patched versions, set previous_version to point to the head (previous version CID)
        if let Some(heads) = &block.heads {
            if let Some(head_cid) = heads.first() {
                schema.previous_version = Some(CollectionSource::new(head_cid.to_string()));
            }
        }

        // Views (collections with a query_select) are non-materialized and carry query metadata.
        // Regular collections are materialized.
        if let Some(ref query_bytes) = payload.query_select {
            schema.is_materialized = false;
            if let Ok(query_value) = serde_json::from_slice::<serde_json::Value>(query_bytes) {
                let mut source = QuerySource::new(query_value);
                if let Some(ref transform_cid) = payload.query_transform {
                    source.transform = Some(transform_cid.to_string());
                }
                schema.query = Some(source);
            } else {
                tracing::warn!(
                    cid = %cid,
                    "Failed to decode query_select JSON bytes for view collection"
                );
            }
        } else if schema.query.is_none() {
            schema.is_materialized = true;
        }

        // Store in systemstore
        let txn = self.db.new_txn(false).await.map_err(MergeError::Database)?;
        {
            let systemstore = txn.systemstore().map_err(MergeError::Database)?;
            schema.root_id = crate::collection::ensure_persisted_collection_short_id(
                &systemstore,
                &collection_id,
            )
            .await
            .map_err(MergeError::Database)?;

            // 1. Store full schema at /collection/id/{version_id}
            let collection_key = CollectionKey::new(&version_id);
            let data = serde_json::to_vec(&schema).map_err(|e| {
                MergeError::Storage(format!("Failed to serialize collection schema: {}", e))
            })?;
            systemstore
                .set(&collection_key.bytes(), &data)
                .await
                .map_err(|e| MergeError::Storage(format!("Failed to store collection: {}", e)))?;

            // 2. Store version index at /collection/version/{collection_id}/{version_id}
            let version_key = CollectionVersionKey::new(&collection_id, &version_id);
            systemstore
                .set(&version_key.bytes(), b"1")
                .await
                .map_err(|e| {
                    MergeError::Storage(format!("Failed to store version index: {}", e))
                })?;
        }
        txn.commit().await.map_err(MergeError::Database)?;

        // Add to runtime cache so it's visible via list_collections/get_collection.
        // Synced collections are inactive but still need to be in the cache for
        // GetCollections with GetInactive=true to find them.
        //
        // Two things can stop the write. The cache refuses a name another
        // collection already holds, which `Cached` reports. Before that, a
        // record committing to what the delta cannot carry is not rebuilt
        // from one: the synced version stays stored under its own version ID
        // either way.
        let uncarried = self
            .db
            .get_collection(&collection_name)
            .map_err(MergeError::Database)?
            .as_ref()
            .map(|existing| uncarried_commitments(existing.schema(), &schema))
            .unwrap_or_default();
        let cached = if uncarried.is_empty() {
            self.db
                .add_collection_to_cache(schema.clone())
                .await
                .map_err(MergeError::Database)?
                == crate::collection::Cached::Taken
        } else {
            tracing::warn!(
                collection_name = %collection_name,
                version_id = %version_id,
                commitments = %uncarried.join(", "),
                "Synced collection definition kept out of the cache: it cannot carry what the \
                 collection of that name already commits to"
            );
            false
        };

        tracing::debug!(
            collection_name = %collection_name,
            version_id = %version_id,
            is_active = schema.is_active,
            is_materialized = schema.is_materialized,
            "Stored synced collection schema"
        );

        tracing::info!(
            collection_name = %collection_name,
            version_id = %version_id,
            field_count = schema.fields.len(),
            cached,
            "Registered synced collection schema in systemstore (inactive, requires manual \
             activation); cached unless the name already holds another collection"
        );

        Ok(MergeOutcome::Merged)
    }

    /// Convert a FieldDefinitionDeltaPayload to a FieldDescription.
    pub(crate) fn field_definition_to_description(
        &self,
        payload: &FieldDefinitionDeltaPayload,
        field_id: &str,
    ) -> Result<FieldDescription, MergeError> {
        let name = payload
            .name
            .clone()
            .unwrap_or_else(|| format!("field_{}", field_id));

        // Determine the FieldKind from the payload
        let kind = if let Some(collection_id) = &payload.collection_id {
            // Relation field
            FieldKind::Relation {
                collection_id: collection_id.clone(),
                is_array: false, // Default; actual value would need additional info
            }
        } else if let Some(relative_id) = payload.relative_id {
            // Self-referencing field
            FieldKind::SelfRef {
                relative_id: relative_id.to_string(),
                is_array: false,
            }
        } else if let Some(scalar_kind_u8) = payload.scalar_kind {
            FieldKind::from_numeric_kind(scalar_kind_u8)
        } else {
            // Default to None scalar
            FieldKind::Scalar(ScalarKind::None)
        };

        // Determine CRDT type
        let crdt_type = payload.crdt.map(CType::from_u8).unwrap_or_default();

        let mut field =
            FieldDescription::new(field_id.to_string(), name, kind).with_crdt_type(crdt_type);
        // Immutability is in the field's own identity, so it round-trips.
        field.immutable = payload.immutable;
        Ok(field)
    }
}

/// The collection ID a definition block's initial version carries.
///
/// The block is hashed over a delta that includes the policy, so its CID is
/// the version ID. The collection ID is the CID of the same delta without it,
/// which is what the author derived and what peers must agree on: a policy
/// mints a new version of the same collection, never a different one.
fn collection_id_of(version_id: &Cid, block: &Block) -> Result<String, MergeError> {
    let CrdtDelta::CollectionDefinition(payload) = &block.delta else {
        return Ok(version_id.to_string());
    };
    if payload.policy_cid.is_none() {
        return Ok(version_id.to_string());
    }
    let mut without_policy = block.clone();
    if let CrdtDelta::CollectionDefinition(payload) = &mut without_policy.delta {
        payload.policy_cid = None;
    }
    without_policy
        .generate_cid()
        .map(|cid| cid.to_string())
        .map_err(|error| MergeError::MergeFailed(error.to_string()))
}

/// What `stored` commits to that `incoming` does not carry.
///
/// A governed collection's delta carries its root, its branchable flag and its
/// fields' immutability, so a rebuilt record restores all three and none of
/// them is a reason to refuse. Three things still are.
///
/// A policy survives only as a CID over its reference, which is enough to bind
/// the version but not to reconstruct the reference itself, so a record
/// holding one must not be rebuilt from a delta.
///
/// A differing governance root means the incoming definition describes a
/// different collection that merely shares a name — their collection IDs
/// differ by construction — and the name-keyed cache would otherwise let it
/// take the local one's place. The same root with a differing collection ID
/// is the same situation: a governed identity also commits to the fields'
/// immutability and to branchability, so a definition under the local root
/// that drops either is a different collection too, and must not displace
/// the record that holds them.
///
/// An ungoverned collection commits to none of this in its identity, so its
/// delta carries neither the immutable flags nor the branchable flag and a
/// rebuild would drop them.
fn uncarried_commitments(
    stored: &CollectionVersion,
    incoming: &CollectionVersion,
) -> Vec<&'static str> {
    let mut commitments = Vec::new();
    if stored.policy.is_some() {
        commitments.push("an access control policy");
    }
    if stored.governance_root != incoming.governance_root {
        commitments.push("a different governance root");
    } else if stored.governance_root.is_some() && stored.collection_id != incoming.collection_id {
        commitments.push("a different collection ID under the same root");
    }
    if stored.governance_root.is_none() {
        if stored.fields.iter().any(|field| field.immutable) {
            commitments.push("@immutable fields");
        }
        if stored.is_branchable {
            commitments.push("branchable history");
        }
    }
    commitments
}
