//! Which rows of an update page its reader may be shown, decided once per
//! document in the transaction that read the page.

use crate::collection::Collection;
use crate::error::Result;
use datastore::NamespaceView;
use query::fetcher::DocumentReader;
use rapidhash::{HashMapExt, RapidHashMap};
use storage::corekv::Store;

pub(super) struct Gate<'page> {
    collection: &'page Collection,
    reader: &'page DocumentReader<'page>,
    datastore: NamespaceView,
    systemstore: NamespaceView,
    decided: RapidHashMap<String, bool>,
}

impl<'page> Gate<'page> {
    pub(super) fn new<S>(
        txn: &crate::txn::DbTxn<S>,
        collection: &'page Collection,
        reader: &'page DocumentReader<'page>,
    ) -> Result<Self>
    where
        S: Store,
    {
        Ok(Self {
            collection,
            reader,
            datastore: txn.datastore()?,
            systemstore: txn.systemstore()?,
            decided: RapidHashMap::new(),
        })
    }

    /// Whether the reader may see `doc_id`'s rows. A collection without a
    /// policy has no document to hide.
    pub(super) async fn admits(&mut self, doc_id: &str) -> Result<bool> {
        let definition = self.collection.schema();
        let Some(policy) = definition.policy.as_ref() else {
            return Ok(true);
        };
        if let Some(&admitted) = self.decided.get(doc_id) {
            return Ok(admitted);
        }
        let admitted = self.alive(doc_id).await?
            && query::txn::check_doc_read_access_as(
                self.reader.context,
                self.reader.acp,
                self.reader.projection,
                self.reader.identity,
                &policy.id,
                &policy.resource_name,
                &definition.collection_id,
                definition.is_branchable,
                doc_id,
                None,
            )
            .await?;
        self.decided.insert(doc_id.to_owned(), admitted);
        Ok(admitted)
    }

    /// Whether the document is neither deleted nor purged. A delete
    /// unregisters the document, and an unregistered document reads as
    /// public, so the access check alone would show its rows to anyone.
    async fn alive(&self, doc_id: &str) -> Result<bool> {
        let short_id = crate::docid::map::get_doc_short_id(
            &self.systemstore,
            self.collection.resolved_root_id(),
            doc_id,
        )
        .await?;
        match short_id {
            Some(short_id) => Ok(!self
                .collection
                .is_deleted(&self.datastore, short_id)
                .await?),
            None => Ok(false),
        }
    }
}
