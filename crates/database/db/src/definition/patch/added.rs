use super::*;

impl<S: Store> crate::database::DB<S> {
    /// Store a version the patch added under a version ID no stored version
    /// has, as Go's `SaveCollection` does: the version itself, its entry in
    /// its collection's version index, and, when it is active, the
    /// collection's name and cache entry.
    ///
    /// Validation has already refused a version of a collection that does not
    /// exist, so `version` belongs to a stored collection. Its writes run in
    /// `txn`, which the caller commits.
    pub(crate) async fn store_added_version(
        &self,
        txn: &mut DbTxn<S>,
        version: &CollectionVersion,
    ) -> Result<()> {
        let version_id = &version.version_id;
        let data = serde_json::to_vec(version).map_err(|e| {
            Error::collection_schema_json(
                format!("failed to serialize added schema version '{version_id}'"),
                e,
            )
        })?;
        {
            let systemstore = txn.systemstore()?;
            systemstore
                .set(&CollectionKey::new(version_id).bytes(), &data)
                .await
                .map_err(Error::Storage)?;
            systemstore
                .set(
                    &CollectionVersionKey::new(&version.collection_id, version_id).bytes(),
                    b"1",
                )
                .await
                .map_err(Error::Storage)?;
            if version.is_active {
                systemstore
                    .set(
                        &CollectionNameKey::new(&version.name).bytes(),
                        version_id.as_bytes(),
                    )
                    .await
                    .map_err(Error::Storage)?;
            }
        }

        if version.is_active {
            let cached_collection =
                Collection::load_index_actions(version.clone(), &txn.systemstore()?).await?;
            let collections = std::sync::Arc::clone(&self.collections);
            let registered_name = version.name.clone();
            txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                collections.publish_named(&registered_name, cached_collection, &[]);
            })))?;
        }

        tracing::info!(
            collection = %version.name,
            version = %version_id,
            is_active = version.is_active,
            "Stored collection version added by patch"
        );
        Ok(())
    }
}
