use super::*;
use serde_json::Value;

/// A patch staged on a transaction, and what it owes once that commits.
pub(crate) struct StagedPatch {
    /// Every version the transaction saw before the patch.
    versions: Vec<CollectionVersion>,
    /// The versions the patch removes.
    removed: Vec<CollectionVersion>,
    /// The in-place updates and new versions it staged.
    outcomes: Vec<PatchOutcome>,
    /// Versions a collection-level move left as they were.
    unchanged: Vec<CollectionVersion>,
    /// Other collections given a one-to-one unique index by the patch.
    indexed: Vec<String>,
}

impl StagedPatch {
    /// Every collection name the patch may have changed what resolves to:
    /// the names it removed or wrote, and the earlier names of each
    /// collection it wrote, so a rename leaves no stale entry behind.
    pub(crate) fn touched_names(&self) -> rapidhash::RapidHashSet<String> {
        let written: rapidhash::RapidHashSet<&str> = self
            .outcomes
            .iter()
            .map(|outcome| outcome.schema.collection_id.as_str())
            .collect();
        self.removed
            .iter()
            .map(|version| version.name.clone())
            .chain(
                self.outcomes
                    .iter()
                    .map(|outcome| outcome.schema.name.clone()),
            )
            .chain(
                self.versions
                    .iter()
                    .filter(|version| written.contains(version.collection_id.as_str()))
                    .map(|version| version.name.clone()),
            )
            .chain(self.indexed.iter().cloned())
            .collect()
    }
}

impl<S: Store> crate::database::DB<S> {
    /// Stage `ops` on `txn`: Go's `PatchCollection` over every collection its
    /// paths name, read and written through `txn` alone, so an explicit
    /// transaction sees its own earlier writes and nobody else sees the patch
    /// before it commits.
    pub(crate) async fn stage_patch(
        &self,
        txn: &mut DbTxn<S>,
        ops: Value,
        migration: Option<lens::LensConfig>,
    ) -> Result<StagedPatch> {
        let versions = self.get_all_collection_versions_with_txn(txn).await?;
        let ops = targets::fold_collection_copies(ops, &versions);
        let groups = targets::group_by_target(&ops, |name| {
            find_target(&versions, name)
                .map(|version| version.version_id.clone())
                .unwrap_or_else(|| name.to_owned())
        })?;
        self.stage_groups(txn, groups, migration).await
    }

    /// Stage each of `groups` on the collection it names: every share is
    /// applied and checked on its own, then staged seeing the others as
    /// patched, removals first.
    pub(crate) async fn stage_groups(
        &self,
        txn: &mut DbTxn<S>,
        groups: Vec<(String, Vec<Value>)>,
        migration: Option<lens::LensConfig>,
    ) -> Result<StagedPatch> {
        let versions = self.get_all_collection_versions_with_txn(txn).await?;
        let mut prepared = Vec::with_capacity(groups.len());
        let mut unchanged = Vec::new();
        for (name, ops) in groups {
            let ops = Value::Array(ops);
            let Some(target) = find_target(&versions, &name) else {
                // A collection-level move is a no-op; anything else is refused.
                unchanged.push(self.handle_unknown_collection_patch(&name, &ops).await?);
                continue;
            };
            prepared.push(
                self.prepare_patch(txn, &name, target, ops, &versions)
                    .await?,
            );
        }

        let deleting: rapidhash::RapidHashSet<&str> = prepared
            .iter()
            .filter(|p| p.is_deactivation)
            .map(|p| p.old_schema.version_id.as_str())
            .collect();
        let removed = match deleting.is_empty() {
            true => Vec::new(),
            false => {
                self.check_versions_removable(txn, &versions, &deleting)
                    .await?
            }
        };
        for version in &removed {
            self.write_version_deletion(txn, version).await?;
        }

        let changes: Vec<&PreparedPatch> = prepared.iter().filter(|p| !p.is_deactivation).collect();
        let mut outcomes = Vec::with_capacity(changes.len());
        let mut indexed = Vec::new();
        for (index, change) in changes.iter().enumerate() {
            let siblings: Vec<CollectionVersion> = changes[index + 1..]
                .iter()
                .map(|later| later.new_schema.clone())
                .collect();
            let outcome = self
                .write_prepared(txn, change, &siblings, migration.clone())
                .await?;
            if outcome.is_new_version && outcome.schema.is_active {
                indexed.extend(
                    self.stage_cross_collection_one_to_one_indexes(txn, &outcome.schema)
                        .await?,
                );
            }
            outcomes.push(outcome);
        }
        Ok(StagedPatch {
            versions,
            removed,
            outcomes,
            unchanged,
            indexed,
        })
    }

    /// The work `staged` owes once its transaction has committed: forbid the
    /// IDs of collections it removed whole, register its migrations, and
    /// rebuild the indexes its new versions owe. None of it can fail the
    /// committed patch. Every version it touched, removed ones marked inactive.
    pub(crate) async fn finish_staged(
        &self,
        staged: StagedPatch,
    ) -> Result<Vec<CollectionVersion>> {
        let StagedPatch {
            versions,
            removed,
            outcomes,
            unchanged,
            indexed: _,
        } = staged;
        let deleting: rapidhash::RapidHashSet<&str> =
            removed.iter().map(|v| v.version_id.as_str()).collect();
        self.forget_removed_collections(&versions, &deleting, &removed)?;
        let mut touched = unchanged;
        touched.extend(removed.iter().cloned().map(|mut version| {
            version.is_active = false;
            version
        }));
        for outcome in outcomes {
            touched.push(self.finish_patch(outcome).await?);
        }
        Ok(touched)
    }
}
