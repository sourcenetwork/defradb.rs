//! The writes of a deferred index (`crate::index::deferred`).

use datastore::NamespaceView;
use document::NormalValue;
use schema::{CollectionVersion, IndexDescription};
use storage::corekv::OnCommit;
use storage::keys::{index_pending_log, index_position_key};

use super::{IndexManager, IndexType};
use crate::index::deferred::{decode_record, encode_record};
use crate::index::error::{Error, Result};

impl IndexManager {
    pub(crate) fn is_deferred(&self, index: &IndexDescription, schema: &CollectionVersion) -> bool {
        crdt::deferred_index(index, schema, &self.crdts)
    }

    /// Owes `index` an update of the document: a blind append, never a
    /// conflict. The drain is asked for once the write commits.
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
        if let Some(drains) = &self.drains {
            let (drains, target) = (drains.clone(), (self.collection_short_id, index.id));
            datastore
                .on_commit(OnCommit::Sync(Box::new(move || drains.request(target))))
                .await
                .map_err(Error::Storage)?;
        }
        Ok(())
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
        let keys = Self::entry_keys(index, doc_short_id, value_sets)?;
        for key in &keys {
            datastore.set(key, &[]).await.map_err(Error::Storage)?;
        }
        datastore
            .set(
                &index_position_key(
                    self.collection_short_id,
                    index.description().id,
                    doc_short_id,
                ),
                &encode_record(&keys)?,
            )
            .await
            .map_err(Error::Storage)
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

    /// The entry keys a position record names.
    pub(crate) fn recorded(bytes: Option<&[u8]>) -> Result<Vec<Vec<u8>>> {
        bytes.map_or(Ok(Vec::new()), decode_record)
    }
}
