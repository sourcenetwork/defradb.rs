//! Which of a collection's indexes take writes and which answer queries.
//!
//! An index whose build or drop is an action still running, or whose build
//! failed, answers no query; one whose build failed takes no writes either.
//! A reindex a schema change owes the collection covers every index: until it
//! settles, the entries may describe documents at an older version or be half
//! rebuilt, so queries plan scans instead. Writers keep maintaining every
//! index that takes writes throughout.

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
}

impl IndexStatuses {
    /// The statuses `collection_id`'s indexes are stored with.
    pub(crate) async fn load(systemstore: &NamespaceView, collection_id: &str) -> Result<Self> {
        Ok(Self {
            reindex_owed: crate::definition::migration::reindex_owed(systemstore, collection_id)
                .await?,
            ..Self::load_actions(systemstore, collection_id).await?
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

    /// [`Self::load_actions`] for `def`, reading nothing when it declares no
    /// index, as [`Self::load_for`].
    pub(crate) async fn load_actions_for(
        systemstore: &NamespaceView,
        def: &CollectionVersion,
    ) -> Result<Self> {
        if def.indexes.is_empty() {
            return Ok(Self::default());
        }
        Self::load_actions(systemstore, &def.collection_id).await
    }

    /// The index actions alone, for a check that compares which indexes take
    /// writes and which are built, where an owed reindex changes nothing.
    pub(crate) async fn load_actions(
        systemstore: &NamespaceView,
        collection_id: &str,
    ) -> Result<Self> {
        Ok(Self::from_actions(
            crate::database::action::index_action_statuses(systemstore, collection_id).await?,
        ))
    }

    /// Statuses with `actions` and no reindex owed.
    pub(crate) fn from_actions(actions: RapidHashMap<u32, ActionStatus>) -> Self {
        Self {
            actions,
            reindex_owed: false,
        }
    }

    /// Whether writers maintain `index`.
    pub(crate) fn takes_writes(&self, index: &IndexDescription) -> bool {
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
