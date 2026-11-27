//! Which of a collection's indexes take writes and which answer queries.
//!
//! An index whose build or drop is an action still running, or whose build
//! failed, answers no query; one whose build failed takes no writes either.
//! A reindex a schema change owes the collection covers every index: until it
//! settles, the entries may describe documents at an older version or be half
//! rebuilt, so queries plan scans instead. Writers keep maintaining every
//! index that takes writes throughout, except the unique index the owed
//! reindex's last rebuild could not fill because a migration gave two
//! documents one of its values: that one is abandoned, as Go abandons a
//! failed index, until a rebuild tries it again
//! (`proofs/tla/ReindexFailureListing.tla`).

use datastore::NamespaceView;
use defra_core::ActionStatus;
use rapidhash::RapidHashMap;
use schema::{CollectionVersion, IndexDescription};

use crate::error::Result;

/// The stored state of a collection's indexes.
#[derive(Debug, Clone, Default)]
pub(crate) struct IndexStatuses {
    actions: RapidHashMap<u32, ActionStatus>,
    reindex_owed: bool,
    /// The unique index the owed reindex's last rebuild could not fill.
    failed_rebuild: Option<u32>,
}

impl IndexStatuses {
    /// The statuses `collection_id`'s indexes are stored with. The owed
    /// record is read validated, so a writer that decided which indexes to
    /// maintain conflicts with a change to it.
    pub(crate) async fn load(systemstore: &NamespaceView, collection_id: &str) -> Result<Self> {
        let owed = crate::definition::migration::owed_status(systemstore, collection_id).await?;
        Ok(Self {
            actions: crate::database::action::index_action_statuses(systemstore, collection_id)
                .await?,
            reindex_owed: owed.is_some(),
            failed_rebuild: owed.flatten(),
        })
    }

    /// [`Self::load`] for `def`. Statuses only decide which of the indexes a
    /// definition declares take writes and answer queries, so one that
    /// declares none reads nothing.
    pub(crate) async fn load_for(
        systemstore: &NamespaceView,
        def: &CollectionVersion,
    ) -> Result<Self> {
        if def.indexes.is_empty() {
            return Ok(Self::default());
        }
        Self::load(systemstore, &def.collection_id).await
    }

    /// Statuses with `actions` and no reindex owed.
    #[cfg(test)]
    pub(crate) fn from_actions(actions: RapidHashMap<u32, ActionStatus>) -> Self {
        Self {
            actions,
            reindex_owed: false,
            failed_rebuild: None,
        }
    }

    /// Statuses with `actions` and the collection's reindex marker.
    pub(crate) fn from_parts(
        actions: RapidHashMap<u32, ActionStatus>,
        reindex_owed: bool,
        failed_rebuild: Option<u32>,
    ) -> Self {
        Self {
            actions,
            reindex_owed,
            failed_rebuild,
        }
    }

    pub(crate) fn failed_rebuild(&self) -> Option<u32> {
        self.failed_rebuild
    }

    /// Whether writers maintain `index`.
    pub(crate) fn takes_writes(&self, index: &IndexDescription) -> bool {
        self.rebuilds(index) && self.failed_rebuild != Some(index.id)
    }

    /// Whether a rebuild fills `index`: every index whose build did not fail,
    /// the one the owed reindex's last rebuild could not fill included.
    pub(crate) fn rebuilds(&self, index: &IndexDescription) -> bool {
        self.actions.get(&index.id) != Some(&ActionStatus::ERRORED)
    }

    /// Whether `index` was built and no action is changing it.
    pub(crate) fn is_built(&self, index: &IndexDescription) -> bool {
        !self.actions.contains_key(&index.id)
    }

    /// Whether a reindex is owed, so that no index answers queries.
    pub(crate) fn reindex_owed(&self) -> bool {
        self.reindex_owed
    }
}
