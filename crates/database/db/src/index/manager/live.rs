//! The live value of a document a deferred index owes, as a backfill indexes it.

use std::sync::Arc;

use datastore::NamespaceView;
use document::{DocID, Document};
use lens::{TargetedHistoryLink, TransformStore};
use rapidhash::RapidHashMap;

use crate::collection::Collection;
use crate::definition::lens::{build_collection_history, versions_have_migrations};
use crate::definition::loader::{
    get_collections_by_collection_id, get_collections_by_collection_id_known,
};
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
    /// For a read only: whether every document reads as stored, so its
    /// version need not be read. Decided on the first document.
    as_stored: Option<bool>,
    for_read: bool,
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
            as_stored: None,
            for_read: false,
        }
    }

    /// [`new`](Self::new) for a read through the index, which decides
    /// nothing on a document's version: with no transform in the
    /// collection's history, documents are read as stored without it.
    pub(crate) fn for_read(
        collection: &'a Collection,
        datastore: &'a NamespaceView,
        systemstore: &'a NamespaceView,
        lens_store: Option<&'a Arc<dyn TransformStore>>,
    ) -> Self {
        Self {
            for_read: true,
            ..Self::new(collection, datastore, systemstore, lens_store)
        }
    }

    /// Whether no document can read other than as stored: there is no lens
    /// store, or no version of the collection names a transform.
    async fn reads_as_stored(&mut self) -> Result<bool> {
        if let Some(as_stored) = self.as_stored {
            return Ok(as_stored);
        }
        let as_stored = match self.lens_store {
            None => true,
            Some(_) => {
                let versions = get_collections_by_collection_id_known(
                    self.systemstore,
                    self.collection.collection_id(),
                    Some(self.collection),
                )
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
                let as_stored = !versions_have_migrations(&versions);
                if !as_stored {
                    self.history =
                        build_collection_history(&versions, self.collection.version_id());
                }
                as_stored
            }
        };
        self.as_stored = Some(as_stored);
        Ok(as_stored)
    }

    /// The document as the transforms on its version's path make it, or
    /// `None` once it is deleted.
    pub(crate) async fn get(&mut self, doc_short_id: u64) -> Result<Option<Document>> {
        let other = |e: crate::error::Error| Error::Other(e.to_string());
        if self.for_read && self.reads_as_stored().await? {
            return self
                .collection
                .read_stored_hint(self.datastore, doc_short_id)
                .await
                .map_err(other);
        }
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
            ToActive::Migrated {
                document: migrated, ..
            } => migrated,
            ToActive::Current | ToActive::Identity => doc,
        }))
    }
}
