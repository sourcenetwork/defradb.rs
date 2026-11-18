//! The live value of a document a deferred index owes, as a backfill indexes it.

use std::sync::Arc;

use datastore::NamespaceView;
use document::{DocID, Document};
use lens::{TargetedHistoryLink, TransformStore};
use rapidhash::RapidHashMap;

use crate::collection::Collection;
use crate::definition::lens::build_collection_history;
use crate::definition::loader::get_collections_by_collection_id;
use crate::definition::migration::helpers::{migrate_to_active, ToActive};
use crate::index::error::{Error, Result};

type History = RapidHashMap<String, TargetedHistoryLink>;

/// Reads owed documents at their collection's active version.
pub(crate) struct LiveDocs<'a> {
    collection: &'a Collection,
    datastore: &'a NamespaceView,
    systemstore: &'a NamespaceView,
    lens_store: Option<&'a Arc<dyn TransformStore>>,
    history: Option<History>,
}

impl<'a> LiveDocs<'a> {
    /// With no `lens_store`, documents are read as stored, as the caller's
    /// own reads of them are.
    pub(crate) fn new(
        collection: &'a Collection,
        datastore: &'a NamespaceView,
        systemstore: &'a NamespaceView,
        lens_store: Option<&'a Arc<dyn TransformStore>>,
    ) -> Self {
        Self {
            collection,
            datastore,
            systemstore,
            lens_store,
            history: None,
        }
    }

    /// The document as the transforms on its version's path make it, or
    /// `None` once it is deleted.
    pub(crate) async fn get(&mut self, doc_short_id: u64) -> Result<Option<Document>> {
        let other = |e: crate::error::Error| Error::Other(e.to_string());
        let Some(mut doc) = self
            .collection
            .read_hint(self.datastore, doc_short_id)
            .await
            .map_err(other)?
        else {
            return Ok(None);
        };
        let Some(lens_store) = self.lens_store else {
            return Ok(Some(doc));
        };
        if doc
            .collection_version_id()
            .is_none_or(|version| version == self.collection.version_id())
        {
            return Ok(Some(doc));
        }
        let history = match self.history.take() {
            Some(history) => history,
            None => {
                let versions = get_collections_by_collection_id(
                    self.systemstore,
                    self.collection.collection_id(),
                )
                .await
                .map_err(other)?;
                build_collection_history(&versions, self.collection.version_id()).ok_or_else(
                    || Error::Other("failed to build the deferred index migration history".into()),
                )?
            }
        };
        // A lens may read the document's id.
        if let Some(id) = crate::docid::map::get_doc_id(self.systemstore, doc_short_id)
            .await
            .map_err(other)?
            .and_then(|id| id.parse::<DocID>().ok())
        {
            doc.set_id(id);
        }
        let migrated = migrate_to_active(&doc, self.collection, &history, lens_store).await;
        self.history = Some(history);
        Ok(Some(match migrated.map_err(other)? {
            ToActive::Migrated(migrated) => migrated,
            ToActive::Current | ToActive::Identity => doc,
        }))
    }
}
