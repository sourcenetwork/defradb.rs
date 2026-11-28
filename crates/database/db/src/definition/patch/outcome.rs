use super::store::StoredVersion;
use crate::collection::ops::DeletedVersion;
use crate::error::Result;
use crate::txn::fence::CommitToken;
use crate::txn::DbTxn;
use storage::corekv::Store;

/// What a patch wrote, for the in-memory follow-up once its transaction
/// commits ([`crate::database::DB::after_patch`]).
pub(crate) struct Patched {
    pub(super) deleted: Vec<DeletedVersion>,
    pub(super) stored: Vec<StoredVersion>,
}

impl<S> crate::database::DB<S>
where
    S: Store,
{
    /// The follow-up to a committed patch: forbid the collections it left
    /// without a version, register the lenses it committed, and rebuild the
    /// indexes its activations owe.
    pub(crate) async fn after_patch(
        &self,
        patched: Patched,
        commit: Option<CommitToken>,
    ) -> Result<()> {
        let Patched { deleted, stored } = patched;
        if stored.is_empty() {
            self.settle_owed_reindexes_with_commit(commit.as_ref())
                .await;
            for deleted in &deleted {
                self.after_version_deleted(deleted)?;
            }
            return Ok(());
        }
        for deleted in &deleted {
            self.after_version_deleted(deleted)?;
        }
        for stored in stored {
            self.after_version_stored(stored, commit.as_ref()).await;
        }
        self.settle_owed_reindexes_with_commit(commit.as_ref())
            .await;
        Ok(())
    }
}

/// Drops every collection `txn` has cached, so its next lookup of each reads
/// the definitions the patch just wrote in `txn`.
pub(crate) fn forget_cached_collections<S>(txn: &DbTxn<S>)
where
    S: Store,
{
    for name in txn.cached_collection_names() {
        txn.uncache_collection(&name);
    }
}
