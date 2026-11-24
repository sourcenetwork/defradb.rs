use super::*;

impl<S: Store> crate::database::DB<S> {
    /// Generate a version ID (CID) from schema content during patching.
    ///
    /// Matches Go DefraDB's saveBlocks() behavior:
    /// - Existing fields (present in old_schema) are SKIPPED entirely
    /// - Only NEW fields get CIDs generated with priority=1 (empty headstore)
    /// - The collection block uses headstore heads and priority
    ///
    /// A field or collection CID that cannot be generated is an error.
    pub(crate) fn generate_patch_version_id_with_heads(
        schema: &mut CollectionVersion,
        old_schema: &CollectionVersion,
        collection_priority: u64,
        collection_heads: &[cid::Cid],
        collection_id_map: &rapidhash::RapidHashMap<String, String>,
    ) -> Result<(String, Option<Vec<u8>>, Option<cid::Cid>)> {
        use cid::Cid;

        // Resolve FieldKind::Named to FieldKind::Relation before CID generation.
        // Go's substituteRelationFieldKinds resolves named kinds to CollectionKind
        // with the actual collection_id before generating field deltas.
        for field in &mut schema.fields {
            if let schema::FieldKind::Named { name, is_array } = &field.kind {
                if let Some(col_id) = collection_id_map.get(name.as_str()) {
                    field.kind = schema::FieldKind::Relation {
                        collection_id: col_id.clone(),
                        is_array: *is_array,
                    };
                }
            }
        }

        // Build set of old field names for detecting which fields are new
        let old_field_names: rapidhash::RapidHashSet<&str> = old_schema
            .fields
            .iter()
            .filter(|f| !f.id.is_empty())
            .map(|f| f.name.as_str())
            .collect();

        // Go's saveBlocks skips fields that already have a FieldID (existing fields).
        // Only NEW fields (not in old_schema) get CID generation and DAGLink inclusion.
        // Go also skips secondary relation fields (those have empty FieldID in old schema too,
        // but Delta returns hasFieldChanged=false for them).
        let new_field_indices: Vec<usize> = {
            let mut indices: Vec<usize> = schema
                .fields
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    // New field: not in old schema and not a secondary relation
                    // (secondary relations have relation_name set and is_primary=false)
                    let is_new = !old_field_names.contains(f.name.as_str());
                    let is_secondary_relation = f.relation_name.is_some() && !f.is_primary;
                    is_new && !is_secondary_relation
                })
                .map(|(i, _)| i)
                .collect();
            // Sort: _docID first, then alphabetically
            indices.sort_by(|&a, &b| {
                let fa = &schema.fields[a];
                let fb = &schema.fields[b];
                if fa.name == "_docID" {
                    std::cmp::Ordering::Less
                } else if fb.name == "_docID" {
                    std::cmp::Ordering::Greater
                } else {
                    fa.name.cmp(&fb.name)
                }
            });
            indices
        };

        // Generate CIDs only for NEW fields with priority=1 (matching Go's empty headstore)
        let governed = schema.governance_root.is_some();
        let mut field_cids: Vec<Cid> = Vec::new();
        for &idx in &new_field_indices {
            let field = &schema.fields[idx];
            let cid = schema::generate_field_cid_with_priority(field, 1, governed)?;
            schema.fields[idx].id = cid.to_string();
            field_cids.push(cid);
        }

        // Generate collection CID with headstore heads.
        // Go's Delta only includes name when it changed. For field-only patches, name=None.
        let name_changed = schema.name != old_schema.name;
        let collection_name = if name_changed {
            Some(schema.name.as_str())
        } else {
            None
        };
        let query_select = schema
            .query
            .as_ref()
            .filter(|query| old_schema.query.as_ref() != Some(query))
            .and_then(|query| schema::query_select_json_bytes(&query.query).ok());
        let query_transform = schema
            .query
            .as_ref()
            .and_then(|query| query.transform.as_deref())
            .filter(|transform| {
                old_schema
                    .query
                    .as_ref()
                    .and_then(|query| query.transform.as_deref())
                    != Some(*transform)
            })
            .and_then(|transform| cid::Cid::try_from(transform).ok());

        let version_id = schema::generate_collection_cid_full_with_query(
            collection_name,
            &field_cids,
            collection_priority,
            collection_heads,
            query_select.as_deref(),
            query_transform.as_ref(),
            schema::Commitments::of(&*schema),
        )?
        .to_string();
        Ok((version_id, query_select, query_transform))
    }
}
