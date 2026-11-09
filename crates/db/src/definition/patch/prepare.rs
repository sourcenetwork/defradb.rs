use super::*;

impl PreparedPatch {
    /// The patch keeps the version it started from: it only toggles
    /// `IsActive`, links a migration transform, or changes metadata. Go
    /// updates such a version in place instead of adding one.
    pub(crate) fn is_in_place(&self) -> bool {
        let (old, new) = (&self.old_schema, &self.new_schema);
        let same_shape = new.fields == old.fields && new.name == old.name;
        if self.is_active_explicitly_set {
            return same_shape;
        }
        same_shape
            && new.is_active == old.is_active
            && (new.previous_version != old.previous_version || new.query == old.query)
    }

    /// The patch only links a migration transform to the version.
    pub(crate) fn is_transform_only(&self) -> bool {
        let (old, new) = (&self.old_schema, &self.new_schema);
        !self.is_active_explicitly_set
            && new.fields == old.fields
            && new.name == old.name
            && new.is_active == old.is_active
            && new.previous_version != old.previous_version
    }
}

impl<S: Store> crate::database::DB<S> {
    /// Apply `patch_ops` to `target` and validate the result on its own.
    /// `versions` are every version `txn` sees. Nothing is written.
    pub(crate) async fn prepare_patch(
        &self,
        txn: &DbTxn<S>,
        collection_name: &str,
        target: &CollectionVersion,
        patch_ops: serde_json::Value,
        versions: &[CollectionVersion],
    ) -> Result<PreparedPatch> {
        let old_schema = target.clone();
        let actual_name = old_schema.name.clone();
        let old_version_id = old_schema.version_id.clone();
        let collection_id = old_schema.collection_id.clone();
        // Collect known collection names for Kind validation
        let known_collection_names: Vec<String> = versions
            .iter()
            .filter(|version| version.is_active)
            .map(|version| version.name.clone())
            .collect();

        // Apply the patch to the schema JSON
        let mut schema_json = serde_json::to_value(&old_schema)
            .map_err(|e| Error::collection_schema_json("failed to serialize schema to JSON", e))?;

        // Normalize JSON to match Go's serialization format before applying patches.
        // Go always serializes struct fields (null for nil pointers, [] for nil slices),
        // but Rust's skip_serializing_if omits them. Patches targeting these paths
        // need the keys to exist for replace/remove operations to work correctly,
        // and for validators to run instead of json_pointer errors.
        // Note: EncryptedIndexes is NOT pre-populated because Go doesn't expose
        // it in the JSON representation - patches targeting it should fail.
        if let serde_json::Value::Object(ref mut map) = schema_json {
            for key in &["Indexes", "VectorEmbeddings"] {
                map.entry(key.to_string())
                    .or_insert(serde_json::Value::Array(vec![]));
            }
            for key in &["CollectionSet", "Query", "PreviousVersion", "Policy"] {
                map.entry(key.to_string())
                    .or_insert(serde_json::Value::Null);
            }
        }

        // Go DefraDB embeds collection name in patch paths: /CollectionName/Fields/-
        // We need to strip the collection name prefix to get paths relative to schema.
        // Patches may use the collection name, actual name, or version ID as prefix.
        let mut strip_prefixes: Vec<String> = vec![format!("/{}/", collection_name)];
        if actual_name != collection_name {
            strip_prefixes.push(format!("/{}/", actual_name));
        }
        if old_version_id != collection_name && old_version_id != actual_name {
            strip_prefixes.push(format!("/{}/", old_version_id));
        }

        let (is_deactivation, is_active_explicitly_set) = self
            .apply_patch_operations(
                patch_ops,
                &mut schema_json,
                &strip_prefixes,
                &known_collection_names,
                &old_schema,
                &collection_id,
            )
            .await?;

        let mut new_schema =
            self.validate_patched_schema(schema_json, &old_schema, &actual_name)?;
        // root_id is runtime storage metadata and is intentionally skipped in schema JSON.
        // Preserve it across patch application so the new active version keeps using the
        // same index/head/cache namespace as the rest of the collection history.
        new_schema.root_id = old_schema.root_id;

        // Go compatibility: default new fields with CType::None to CType::LwwRegister.
        // Go's patchCollection does this in collection_define.go for new fields that
        // don't have an explicit CRDT type. This must happen before CID generation.
        {
            let old_field_names: rapidhash::RapidHashSet<&str> =
                old_schema.fields.iter().map(|f| f.name.as_str()).collect();
            for field in &mut new_schema.fields {
                if !old_field_names.contains(field.name.as_str())
                    && !field.kind.is_relation()
                    && field.crdt_type == schema::CType::None
                {
                    field.crdt_type = schema::CType::LwwRegister;
                }
            }
        }

        // Matches Go's validateDematerializedViewHasNoData.
        if old_schema.is_materialized
            && !new_schema.is_materialized
            && self.collection_has_data(txn, &old_schema).await?
        {
            return Err(Error::InvalidPatch(format!(
                "cannot dematerialize a materialized view that has data, first truncate it and then try again. Name: {}, VersionID: {}",
                old_schema.name, old_schema.version_id
            )));
        }

        Ok(PreparedPatch {
            collection_name: collection_name.to_owned(),
            old_schema,
            new_schema,
            is_deactivation,
            is_active_explicitly_set,
        })
    }
}
