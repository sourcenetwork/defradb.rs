use super::*;
use serde_json::Value;

impl<S: Store> crate::database::DB<S> {
    /// Go's `PatchCollection` over several collections: each group of
    /// `groups` is applied to the collection it names, every result is
    /// validated against the others, and every change (removed versions,
    /// in-place updates, new versions) commits in one transaction, or none.
    pub(crate) async fn patch_many_as(
        &self,
        context: &defra_core::RequestContext,
        groups: Vec<(String, Vec<Value>)>,
        migration: Option<lens::LensConfig>,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;

        let mut prepared = Vec::with_capacity(groups.len());
        for (name, ops) in groups {
            let ops = Value::Array(ops);
            let Some(collection) = self.find_patch_target(&name).await? else {
                // A collection-level move is a no-op; anything else is refused.
                self.handle_unknown_collection_patch(&name, &ops).await?;
                continue;
            };
            prepared.push(self.prepare_patch(&name, &collection, ops).await?);
        }

        let all_versions = self.get_all_collection_versions().await?;
        let deleting: rapidhash::RapidHashSet<&str> = prepared
            .iter()
            .filter(|p| p.is_deactivation)
            .map(|p| p.old_schema.version_id.as_str())
            .collect();
        let removed = match deleting.is_empty() {
            true => Vec::new(),
            false => {
                self.check_versions_removable(&all_versions, &deleting)
                    .await?
            }
        };
        let changes: Vec<&PreparedPatch> = prepared.iter().filter(|p| !p.is_deactivation).collect();

        let mut txn = self.new_txn(false).await?;
        for version in &removed {
            self.write_version_deletion(&txn, version).await?;
        }
        let mut outcomes = Vec::with_capacity(changes.len());
        for (index, change) in changes.iter().enumerate() {
            let siblings: Vec<CollectionVersion> = changes[index + 1..]
                .iter()
                .map(|later| later.new_schema.clone())
                .collect();
            outcomes.push(
                self.write_prepared(&mut txn, change, &siblings, migration.clone())
                    .await?,
            );
        }
        txn.commit().await?;

        self.forget_removed_collections(&all_versions, &deleting, &removed)?;
        for outcome in outcomes {
            self.finish_patch(outcome).await?;
        }
        Ok(())
    }
}
