//! One page of a reindex, in its own engine transaction.
//!
//! A page first checks that the reindex it runs is still the one owed, then
//! carries the stage on from where the last committed page left it, at most
//! `size` items: index keys removed while clearing, documents indexed again
//! while walking. The page that walks past the last document trains what
//! must be trained and clears the owed record with its progress.

use std::sync::Arc;

use async_trait::async_trait;
use datastore::{BasicTxn, NamespaceView};
use lens::TransformStore;
use rapidhash::{HashMapExt, RapidHashMap};

use super::owed;
use super::progress::{self, Stage};
use crate::collection::epoch::EpochedCollections;
use crate::collection::Collection;
use crate::definition::lens::build_collection_history;
use crate::definition::loader::get_collections_by_collection_id;
use crate::definition::migration::helpers::{cache_migrated_document, migrate_to_active, ToActive};
use crate::error::{Error, Result};
use crate::index::manager::DocumentSource;
use crate::index::pages::Page;
use crate::index::IndexManager;
use crate::BackfillSource;

/// What a committed page did.
pub(super) enum Step {
    /// More is left. `needs_fence`: the walk has begun and the next page
    /// needs a fence taken after the clear committed.
    More {
        materialized: usize,
        needs_fence: bool,
    },
    /// The last page committed and cleared the owed record.
    Done { materialized: usize },
    /// A later schema change owes the collection a newer reindex, which its
    /// own settle runs.
    Superseded,
}

pub(super) struct ReindexPage {
    pub(super) collection: Collection,
    pub(super) token: u64,
    /// Whether this page records the owed reindex itself, for a rebuild
    /// asked for directly while none was owed.
    pub(super) owe: bool,
    /// A fence taken after the clear committed, for a walk that has none.
    pub(super) fence: Option<u64>,
    /// Also move the version of documents whose path runs no transform.
    pub(super) restamp_identity: bool,
    pub(super) lens_store: Arc<dyn TransformStore>,
    pub(super) collections: Arc<EpochedCollections>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Page for ReindexPage {
    type Output = Step;

    async fn run(&mut self, txn: &mut BasicTxn, size: usize) -> Result<Step> {
        self.step(&txn.datastore(), &txn.systemstore(), size).await
    }
}

impl ReindexPage {
    async fn step(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        size: usize,
    ) -> Result<Step> {
        let collection_id = self.collection.collection_id();
        if self.owe {
            owed::owe_full_reindex(systemstore, &self.collections, collection_id, self.token)
                .await?;
        } else if !owed::owes(systemstore, collection_id, self.token).await? {
            return Ok(Step::Superseded);
        }
        let collection = crate::collection::loader::load_collection_for_write_attempt(
            systemstore,
            self.collection.name(),
            &self.collection,
        )
        .await?;
        let manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
            collection.crdts(),
        )?;

        let mut stage = progress::read(systemstore, collection_id, self.token).await?;
        let mut budget = size;
        let clearing = matches!(stage, Stage::Clear { .. });
        clear(&manager, datastore, &mut stage, &mut budget).await?;
        let Stage::Walk { fence, last } = stage else {
            progress::write(systemstore, collection_id, self.token, stage).await?;
            return Ok(Step::More {
                materialized: 0,
                needs_fence: false,
            });
        };
        // The page that finished the clear walks what its snapshot holds; the
        // pages after it stop at a fence taken once that page committed, so a
        // document created since is its writer's to index.
        let fence = match (fence, self.fence) {
            (Some(fence), _) => Some(fence),
            (None, _) if clearing => None,
            (None, Some(fence)) => Some(fence),
            (None, None) => {
                return Ok(Step::More {
                    materialized: 0,
                    needs_fence: true,
                })
            }
        };
        let walked = self
            .walk(
                &collection,
                &manager,
                datastore,
                systemstore,
                fence,
                last,
                budget,
            )
            .await?;
        if walked.exhausted {
            for index in manager.get_indexes() {
                manager.build_index(datastore, &index.name).await?;
            }
            owed::clear_owed_reindex(systemstore, &self.collections, collection_id, self.token)
                .await?;
            return Ok(Step::Done {
                materialized: walked.materialized,
            });
        }
        progress::write(
            systemstore,
            collection_id,
            self.token,
            Stage::Walk {
                fence,
                last: walked.last,
            },
        )
        .await?;
        Ok(Step::More {
            materialized: walked.materialized,
            needs_fence: fence.is_none(),
        })
    }

    /// Indexes again, in every index that takes writes, up to `budget`
    /// documents after `last` and below `fence`, each migrated to the active
    /// version and persisted first.
    #[allow(clippy::too_many_arguments)]
    async fn walk(
        &self,
        collection: &Collection,
        manager: &IndexManager,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        fence: Option<u64>,
        mut last: u64,
        budget: usize,
    ) -> Result<Walked> {
        let versions =
            get_collections_by_collection_id(systemstore, collection.collection_id()).await?;
        let history =
            build_collection_history(&versions, collection.version_id()).ok_or_else(|| {
                Error::Lens(format!(
                    "failed to build migration history for collection '{}'",
                    collection.name()
                ))
            })?;
        let mut source = BackfillSource::open_range(
            collection.clone(),
            datastore.clone(),
            systemstore.clone(),
            (last > 0).then_some(last),
            fence,
        )
        .await?;
        // The unique keys each document this page migrated held before, so a
        // unique value a migration gave a document is told from one it had.
        let mut original_keys = RapidHashMap::new();
        let mut materialized = 0;
        for _ in 0..budget {
            let Some((doc_short_id, doc)) = source.next().await? else {
                return Ok(Walked {
                    materialized,
                    last,
                    exhausted: true,
                });
            };
            if doc_short_id == 0 {
                continue;
            }
            last = doc_short_id;
            let doc = match migrate_to_active(&doc, collection, &history, &self.lens_store).await? {
                ToActive::Current => doc,
                ToActive::Identity if self.restamp_identity => {
                    if cache_migrated_document(datastore, systemstore, collection, &doc).await? {
                        materialized += 1;
                    }
                    let mut restamped = doc;
                    restamped.set_collection_version_id(collection.version_id());
                    restamped
                }
                ToActive::Identity => doc,
                ToActive::Migrated(migrated) => {
                    original_keys.insert(
                        doc_short_id,
                        manager.unique_index_keys(&doc, collection.schema())?,
                    );
                    if cache_migrated_document(datastore, systemstore, collection, &migrated)
                        .await?
                    {
                        materialized += 1;
                    }
                    migrated
                }
            };
            manager
                .reindex_document(
                    datastore,
                    systemstore,
                    doc_short_id,
                    &doc,
                    collection.schema(),
                    &original_keys,
                )
                .await?;
        }
        Ok(Walked {
            materialized,
            last,
            exhausted: false,
        })
    }
}

struct Walked {
    materialized: usize,
    last: u64,
    exhausted: bool,
}

/// Removes the entries of the indexes in `manager`, in index ID order, up to
/// `budget` keys, and moves `stage` to the walk once none is left.
async fn clear(
    manager: &IndexManager,
    datastore: &NamespaceView,
    stage: &mut Stage,
    budget: &mut usize,
) -> Result<()> {
    let mut indexes = manager.get_indexes();
    indexes.sort_by_key(|index| index.id);
    while let Stage::Clear { next_index, after } = stage {
        let Some(index) = indexes
            .iter()
            .find(|index| u64::from(index.id) >= *next_index)
        else {
            *stage = Stage::Walk {
                fence: None,
                last: 0,
            };
            break;
        };
        if *budget == 0 {
            break;
        }
        let id = u64::from(index.id);
        let from = if id == *next_index {
            after.take()
        } else {
            None
        };
        let limit = *budget;
        let (removed, last) = manager
            .get_index(&index.name)
            .ok_or_else(|| Error::Other(format!("index '{}' not found", index.name)))?
            .remove_page(&mut datastore.clone(), from.as_deref(), limit)
            .await
            .map_err(Error::Storage)?;
        *budget -= removed.min(limit);
        *stage = if removed < limit {
            Stage::Clear {
                next_index: id + 1,
                after: None,
            }
        } else {
            Stage::Clear {
                next_index: id,
                after: last,
            }
        };
    }
    Ok(())
}
