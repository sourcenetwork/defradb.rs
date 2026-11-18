//! Rebuilding a collection's secondary indexes after a schema change, a page
//! per engine transaction.
//!
//! A schema change owes its collection a reindex ([`owed`]). The rebuild runs
//! as a chain of pages ([`page`]), each holding at most a page of work, so no
//! transaction grows with the collection:
//! - the clear removes every entry of each index that takes writes, a page of
//!   keys at a time, entries in an older key encoding included;
//! - the walk visits the documents in short ID order, migrates each one stored
//!   at an older version through the transforms on its path, persists it, and
//!   indexes it again (`IndexManager::reindex_document`);
//! - the page that walks past the last document gives vector indexes their
//!   chance to train and clears the owed record.
//!
//! Each page commits its cursor with its work ([`progress`]), so a rebuild a
//! failure or a crash stops resumes after the last page that committed, the
//! next time the reindex is settled (after the next schema change, at every
//! open) or asked for. Until the last page commits, the owed record stands and
//! no index of the collection answers queries, which plan scans instead.
//!
//! Writers keep every index that takes writes current for their own documents
//! throughout. A document whose entries the clear removed was committed before
//! the clear's last page, so its short ID is below the walk's fence, taken
//! once that page committed; one created later is its writer's to index.
//!
//! What it costs: while a reindex is owed, a query over the collection scans
//! it, and a vector search over it is refused as during an index backfill. A
//! unique index is enforced against entries the walk has put back so far, so
//! a duplicate a writer slips in during that window meets the walk, which
//! settles it the way a merge does (the deterministic winner) and reports it.

mod owed;
mod page;
mod progress;

use std::sync::Arc;

use storage::corekv::Store;

use crate::definition::loader::get_collections_by_collection_id;
use crate::error::{Error, Result};
use crate::index::pages::PAGE_ITEMS;
use crate::DB;

pub use owed::ReindexReport;
pub(crate) use owed::{owe_reindex, reindex_owed};
use page::{ReindexPage, Step};

impl<S> DB<S>
where
    S: Store,
{
    /// Whether a schema change leaves `collection`'s secondary indexes to
    /// rebuild: it has indexes and, for a migration (`migration_destination`),
    /// the active version's history contains the migration's destination, an
    /// ancestor version included. A version switch always needs it.
    pub(super) async fn reindex_needed(
        &self,
        collection: &crate::collection::Collection,
        migration_destination: Option<&str>,
    ) -> Result<bool> {
        if collection.get_indexes().is_empty() {
            return Ok(false);
        }
        let Some(destination) = migration_destination else {
            return Ok(true);
        };
        let read_txn = self.new_txn(true).await?;
        let versions = {
            let systemstore = read_txn.systemstore()?;
            get_collections_by_collection_id(&systemstore, collection.collection_id()).await?
        };
        read_txn.discard()?;
        Ok(
            crate::definition::lens::build_collection_history(&versions, collection.version_id())
                .is_some_and(|history| history.contains_key(destination)),
        )
    }

    /// Rebuild a collection's indexes for the reindex owed to it under `token`.
    pub(super) async fn rebuild_settling(&self, collection_name: &str, token: u64) -> Result<()> {
        self.reindex_in_pages(collection_name, Some(token), false, None)
            .await
            .map(|_| ())
    }

    /// Rebuild secondary indexes for a collection using lens-migrated documents.
    ///
    /// Documents that cross a registered transform are also written back to the
    /// datastore so the rebuilt indexes and cached values cannot diverge. A
    /// rebuild that stops part way stays owed and resumes from its last page.
    pub async fn reindex_collection_with_migrations(&self, collection_name: &str) -> Result<()> {
        if self.get_collection(collection_name)?.is_none() {
            return Ok(());
        }
        self.reindex_in_pages(collection_name, None, false, None)
            .await
            .map(|_| ())
    }

    /// Rebuilds only the indexes `indexes` names, the same paged way.
    pub(crate) async fn reindex_indexes(
        &self,
        collection_name: &str,
        indexes: Vec<u32>,
    ) -> Result<()> {
        self.reindex_in_pages(collection_name, None, false, Some(indexes))
            .await
            .map(|_| ())
    }

    /// [`Self::materialize_collection_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn materialize_collection(&self, collection_name: &str) -> Result<usize> {
        self.materialize_collection_as(&defra_core::RequestContext::ambient(), collection_name)
            .await
    }

    /// Eagerly advance every known-version document in a collection to its active version.
    ///
    /// This writes only the datastore blob, version key, and secondary indexes. It does not
    /// create CRDT commits, update heads, emit mutation events, or gossip data to peers.
    ///
    /// Returns the number of documents whose stored version was advanced.
    pub async fn materialize_collection_as(
        &self,
        context: &defra_core::RequestContext,
        collection_name: &str,
    ) -> Result<usize> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        self.reindex_in_pages(collection_name, None, true, None)
            .await
    }

    /// Runs the reindex of `collection_name` to its last page: the one owed
    /// under `settles`, or for a rebuild of `indexes` (all, for `None`) asked
    /// for directly, the one already owed if it covers them, else a new one
    /// its first page records. Returns how many documents had their stored
    /// version advanced.
    async fn reindex_in_pages(
        &self,
        collection_name: &str,
        settles: Option<u64>,
        restamp_identity: bool,
        indexes: Option<Vec<u32>>,
    ) -> Result<usize> {
        let collection = self
            .get_collection(collection_name)?
            .ok_or_else(|| Error::CollectionNotFound(collection_name.to_string()))?;
        let (token, mut owe, indexes) = match settles {
            Some(token) => (token, false, None),
            None => match self.owed(collection.collection_id()).await? {
                Some(owed) if covers(owed.indexes.as_deref(), indexes.as_deref()) => {
                    (owed.token, false, None)
                }
                // Started over for both sets: the owed one's progress may
                // have passed what this one adds.
                Some(owed) => (rand::random(), true, union(owed.indexes, indexes)),
                None => (rand::random(), true, indexes),
            },
        };
        let mut size = PAGE_ITEMS;
        let mut fence = None;
        let mut materialized = 0;
        loop {
            let page = ReindexPage {
                collection: collection.clone(),
                token,
                owe,
                indexes: indexes.clone(),
                fence,
                restamp_identity,
                lens_store: Arc::clone(&self.lens_store),
                collections: Arc::clone(&self.collections),
            };
            match self.run_page(page, &mut size).await? {
                Step::More {
                    materialized: count,
                    needs_fence,
                } => {
                    materialized += count;
                    owe = false;
                    if needs_fence && fence.is_none() {
                        fence = Some(self.reserve_doc_short_id_fence().await?);
                    }
                }
                Step::Done {
                    materialized: count,
                } => {
                    tracing::debug!(
                        collection = %collection_name,
                        materialized = materialized + count,
                        restamp_identity,
                        "rebuilt the indexes of a collection"
                    );
                    return Ok(materialized + count);
                }
                Step::Superseded => {
                    tracing::debug!(
                        collection = %collection_name,
                        "a later schema change owes the reindex this rebuild was running"
                    );
                    return Ok(materialized);
                }
            }
        }
    }

    /// The reindex owed to `collection_id`, if one is.
    async fn owed(&self, collection_id: &str) -> Result<Option<owed::OwedReindex>> {
        let txn = self.new_txn(true).await?;
        let owed = owed::read_owed(&txn.systemstore()?, collection_id).await?;
        txn.discard()?;
        Ok(owed)
    }
}

/// Whether a reindex of `owed` (every index, for `None`) rebuilds `wanted`.
fn covers(owed: Option<&[u32]>, wanted: Option<&[u32]>) -> bool {
    match (owed, wanted) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(owed), Some(wanted)) => wanted.iter().all(|id| owed.contains(id)),
    }
}

fn union(owed: Option<Vec<u32>>, wanted: Option<Vec<u32>>) -> Option<Vec<u32>> {
    let (mut owed, wanted) = (owed?, wanted?);
    owed.extend(
        wanted
            .into_iter()
            .filter(|id| !owed.contains(id))
            .collect::<Vec<_>>(),
    );
    Some(owed)
}
