//! Which rows of an update page its reader may be shown, decided once per
//! document.

use crate::collection::Collection;
use crate::error::{Error, Result};
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
    /// A gate that reads its documents in `txn`, the page's transaction.
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

    /// Decide whether the reader may see `doc_id`'s rows, unless that is
    /// decided already. A collection without a policy has no document to hide.
    pub(super) async fn decide(&mut self, doc_id: &str) -> Result<()> {
        let definition = self.collection.schema();
        let Some(policy) = definition.policy.as_ref() else {
            return Ok(());
        };
        if self.decided.contains_key(doc_id) {
            return Ok(());
        }
        let admitted = alive(self.collection, &self.datastore, &self.systemstore, doc_id).await?
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
        Ok(())
    }

    /// Hide what a provider outside the page's transaction admitted and a
    /// delete has since removed. Such a provider loses a document's
    /// registration to a delete on this node only after that delete commits
    /// here, and may then read the document as public: a snapshot taken after
    /// it decided holds every such delete. A registration a peer took from a
    /// provider the nodes share is not this node's delete: the document is
    /// still alive here, for this page as for every other query.
    ///
    /// The snapshot is `db`'s when the fetcher has its database, and
    /// otherwise one on the store `page` was begun on.
    pub(super) async fn confirm<S>(
        &mut self,
        db: Option<&crate::database::DB<S>>,
        page: &crate::txn::DbTxn<S>,
    ) -> Result<()>
    where
        S: Store,
    {
        if !self.reader.acp.unregistered_documents_are_public()
            || !self.decided.values().any(|admitted| *admitted)
        {
            return Ok(());
        }
        let later = match db {
            Some(db) => db.new_txn(true).await?,
            None => page.later().await?.ok_or_else(|| {
                Error::Acp(
                    "no database began this transaction, so what a document access provider \
                     outside it admitted cannot be confirmed: read `_documentUpdates` in a \
                     transaction from `DB::new_txn`"
                        .into(),
                )
            })?,
        };
        let confirmed = self.still_alive(&later).await;
        let discarded = later.discard();
        confirmed?;
        discarded
    }

    async fn still_alive<S>(&mut self, later: &crate::txn::DbTxn<S>) -> Result<()>
    where
        S: Store,
    {
        let (datastore, systemstore) = (later.datastore()?, later.systemstore()?);
        for (doc_id, admitted) in &mut self.decided {
            if *admitted {
                *admitted = alive(self.collection, &datastore, &systemstore, doc_id).await?;
            }
        }
        Ok(())
    }

    /// Whether the reader may see `doc_id`'s rows, once decided.
    pub(super) fn admits(&self, doc_id: &str) -> bool {
        self.collection.schema().policy.is_none()
            || self.decided.get(doc_id).copied().unwrap_or(false)
    }
}

/// Whether the document is neither deleted nor purged in the snapshot the
/// stores read. A delete unregisters the document, and an unregistered
/// document reads as public, so the access check alone would show its rows to
/// anyone. The page decides nothing its transaction writes, so neither read
/// is validated: a later delete or purge never fails that transaction.
async fn alive(
    collection: &Collection,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    doc_id: &str,
) -> Result<bool> {
    let Some(short_id) =
        crate::docid::map::peek_doc_short_id(systemstore, collection.resolved_root_id(), doc_id)
            .await?
    else {
        return Ok(false);
    };
    Ok(datastore
        .get_parts(&collection.deleted_key(short_id), &[])
        .await?
        .is_none())
}
