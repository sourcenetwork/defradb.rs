//! The writes of a deferred index (`crate::index::deferred`).

use std::sync::atomic::Ordering;

use datastore::NamespaceView;
use document::NormalValue;
use schema::{CollectionVersion, IndexDescription};
use storage::corekv::OnCommit;
use storage::keys::{index_pending_log, index_position_key};

use super::{IndexManager, IndexType};
use crate::index::deferred::{decode_record, encode_record, Frame};
use crate::index::error::{Error, Result};

impl IndexManager {
    pub(crate) fn is_deferred(&self, index: &IndexDescription, schema: &CollectionVersion) -> bool {
        crdt::deferred_index(index, schema, &self.crdts)
    }

    /// Owes `index` an update of the document: a blind append, never a
    /// conflict. The drain is asked for once the write commits, by one
    /// callback however many documents the write owes.
    pub(crate) async fn owe(
        &self,
        datastore: &NamespaceView,
        index: &IndexDescription,
        doc_short_id: u64,
    ) -> Result<()> {
        datastore
            .append(
                &index_pending_log(self.collection_short_id, index.id),
                &doc_short_id.to_be_bytes(),
                None,
            )
            .await
            .map_err(Error::Storage)?;
        if let Some(drains) = self.drains.as_ref().filter(|_| self.first_owe(index.id)) {
            let (drains, target) = (drains.clone(), (self.collection_short_id, index.id));
            datastore
                .on_commit(OnCommit::Sync(Box::new(move || drains.request(target))))
                .await
                .map_err(Error::Storage)?;
        }
        Ok(())
    }

    /// Whether this is the manager's first owe of `index`. An index it was
    /// not built with always counts as first.
    fn first_owe(&self, index: u32) -> bool {
        self.asked
            .iter()
            .find(|(id, _)| *id == index)
            .is_none_or(|(_, asked)| !asked.swap(true, Ordering::Relaxed))
    }

    /// The keys of the entries `index` holds for a document with these values.
    pub(crate) fn entry_keys(
        index: &IndexType,
        doc_short_id: u64,
        value_sets: &[Vec<NormalValue>],
    ) -> Result<Vec<Vec<u8>>> {
        let IndexType::Simple(simple) = index else {
            return Err(Error::Other(format!(
                "index '{}' is deferred but not a simple index",
                index.description().name
            )));
        };
        value_sets
            .iter()
            .map(|values| simple.entry_key(values, doc_short_id))
            .collect::<storage::corekv::Result<_>>()
            .map_err(Error::Storage)
    }

    /// Writes a document's entries and its position record. Only a writer
    /// that knows nothing else holds them may: a create, or a backfill that
    /// found no record.
    pub(crate) async fn place(
        &self,
        datastore: &NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        value_sets: &[Vec<NormalValue>],
    ) -> Result<()> {
        let prefix = storage::keys::IndexDataStoreKey::index_prefix(
            self.collection_short_id,
            index.description().id,
        );
        self.place_with_prefix(datastore, index, doc_short_id, value_sets, &prefix)
            .await
    }

    async fn place_with_prefix(
        &self,
        datastore: &NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        value_sets: &[Vec<NormalValue>],
        prefix: &[u8],
    ) -> Result<()> {
        let keys = Self::entry_keys(index, doc_short_id, value_sets)?;
        for key in &keys {
            datastore.set(key, &[]).await.map_err(Error::Storage)?;
        }
        let id = index.description().id;
        datastore
            .set(
                &index_position_key(self.collection_short_id, id, doc_short_id),
                &encode_record(&Frame::new(prefix, doc_short_id), &keys)?,
            )
            .await
            .map_err(Error::Storage)
    }

    /// A rebuild's write of one document after the clear. A drain may have
    /// moved the entry since, so the keys the record names go first. The
    /// record is read unvalidated: the put below conflicts with a drain that
    /// moved it anywhere else.
    pub(crate) async fn replace(
        &self,
        datastore: &NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        value_sets: &[Vec<NormalValue>],
    ) -> Result<()> {
        let record = index_position_key(
            self.collection_short_id,
            index.description().id,
            doc_short_id,
        );
        let prefix = storage::keys::IndexDataStoreKey::index_prefix(
            self.collection_short_id,
            index.description().id,
        );
        let old = Self::recorded(
            &Frame::new(&prefix, doc_short_id),
            datastore
                .get_parts(&record, &[])
                .await
                .map_err(Error::Storage)?
                .as_deref(),
        )?;
        let new = Self::entry_keys(index, doc_short_id, value_sets)?;
        for key in old
            .iter()
            .filter(|key| key.starts_with(&prefix) && !new.contains(key))
        {
            datastore.delete(key).await.map_err(Error::Storage)?;
        }
        self.place_with_prefix(datastore, index, doc_short_id, value_sets, &prefix)
            .await
    }

    /// A backfill of one document: placed unless a drain or a create got
    /// there first. The record read decides, so a drain racing this write
    /// makes one of them run again.
    pub(crate) async fn backfill(
        &self,
        datastore: &NamespaceView,
        index: &IndexType,
        doc_short_id: u64,
        value_sets: &[Vec<NormalValue>],
    ) -> Result<()> {
        let record = index_position_key(
            self.collection_short_id,
            index.description().id,
            doc_short_id,
        );
        if datastore
            .get(&record)
            .await
            .map_err(Error::Storage)?
            .is_some()
        {
            return Ok(());
        }
        self.place(datastore, index, doc_short_id, value_sets).await
    }

    /// The entry keys a position record of the document `frame` frames names.
    pub(crate) fn recorded(frame: &Frame<'_>, bytes: Option<&[u8]>) -> Result<Vec<Vec<u8>>> {
        bytes.map_or(Ok(Vec::new()), |bytes| decode_record(frame, bytes))
    }
}
