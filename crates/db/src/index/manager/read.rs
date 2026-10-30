//! Reading through an index, with a deferred index's owed documents checked
//! live (`crate::index::deferred`).

use datastore::NamespaceView;
use document::NormalValue;
use rapidhash::{HashSetExt, RapidHashSet};
use schema::IndexDescription;
use storage::corekv::{IterOptions, MaybeSend, Reader};
use storage::index::{Bound, ExactMatchIterator, IndexOverlay, OverlayReader, RangeIterator};
use storage::keys::{index_pending_log, index_position_key, IndexDataStoreKey};

use super::{IndexManager, IndexType};
use crate::collection::Collection;
use crate::index::error::{Error, Result};

/// An index as a read sees it.
pub struct IndexRead<'a> {
    index: &'a IndexType,
    overlay: Option<IndexOverlay>,
}

impl IndexManager {
    /// The index `name` as a read on `datastore` sees it: a deferred index
    /// hides the stored entries of the documents it still owes and shows their
    /// live entries instead, including those this transaction owes.
    pub async fn read_index(
        &self,
        datastore: &NamespaceView,
        collection: &Collection,
        name: &str,
    ) -> Result<Option<IndexRead<'_>>> {
        let Some(index) = self.indexes.get(name) else {
            return Ok(None);
        };
        let desc = index.description();
        if !self.is_deferred(desc, collection.schema()) {
            return Ok(Some(IndexRead {
                index,
                overlay: None,
            }));
        }
        let owed = self.owed(datastore, desc).await?;
        if owed.is_empty() {
            return Ok(Some(IndexRead {
                index,
                overlay: None,
            }));
        }
        let (mut hidden, mut added) = (Vec::new(), Vec::new());
        for doc in owed {
            let record = index_position_key(self.collection_short_id, desc.id, doc);
            hidden.extend(Self::recorded(
                datastore
                    .get(&record)
                    .await
                    .map_err(Error::Storage)?
                    .as_deref(),
            )?);
            let live = collection
                .read_hint(datastore, doc)
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
            if let Some(live) = live {
                let values = self.extract_index_values(&live, desc, collection.schema())?;
                added.extend(Self::entry_keys(index, doc, &values)?);
            }
        }
        let prefix = IndexDataStoreKey::index_prefix(self.collection_short_id, desc.id);
        Ok(Some(IndexRead {
            index,
            overlay: Some(IndexOverlay::new(prefix, hidden, added)),
        }))
    }

    /// The documents `index` owes an update: those in its log, and those this
    /// transaction appended, which have no position yet.
    async fn owed(&self, datastore: &NamespaceView, index: &IndexDescription) -> Result<Vec<u64>> {
        let log = index_pending_log(self.collection_short_id, index.id);
        let mut owed = RapidHashSet::new();
        let mut iter = datastore
            .iterator(IterOptions::default().with_prefix(log.entry_prefix().to_vec()))
            .await
            .map_err(Error::Storage)?;
        while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
            owed.insert(doc_short_id(&pair.value)?);
        }
        iter.close().await.map_err(Error::Storage)?;
        for entry in datastore.appended(&log).await.map_err(Error::Storage)? {
            owed.insert(doc_short_id(&entry)?);
        }
        Ok(owed.into_iter().collect())
    }
}

fn doc_short_id(entry: &[u8]) -> Result<u64> {
    entry
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| Error::Other("deferred index log entry is not a document short id".into()))
}

impl IndexRead<'_> {
    pub fn description(&self) -> &IndexDescription {
        self.index.description()
    }

    pub fn index(&self) -> &IndexType {
        self.index
    }

    pub async fn get<R>(
        &self,
        txn: &R,
        values: &[NormalValue],
    ) -> storage::corekv::Result<ExactMatchIterator>
    where
        R: Reader + MaybeSend,
    {
        match &self.overlay {
            Some(overlay) => {
                self.index
                    .get(&OverlayReader::new(txn, overlay), values)
                    .await
            }
            None => self.index.get(txn, values).await,
        }
    }

    pub async fn scan<R>(&self, txn: &R, reverse: bool) -> storage::corekv::Result<RangeIterator>
    where
        R: Reader + MaybeSend,
    {
        match &self.overlay {
            Some(overlay) => {
                self.index
                    .scan(&OverlayReader::new(txn, overlay), reverse)
                    .await
            }
            None => self.index.scan(txn, reverse).await,
        }
    }

    pub async fn scan_prefix<R>(
        &self,
        txn: &R,
        prefix_values: &[NormalValue],
        reverse: bool,
    ) -> storage::corekv::Result<RangeIterator>
    where
        R: Reader + MaybeSend,
    {
        match &self.overlay {
            Some(overlay) => {
                self.index
                    .scan_prefix(&OverlayReader::new(txn, overlay), prefix_values, reverse)
                    .await
            }
            None => self.index.scan_prefix(txn, prefix_values, reverse).await,
        }
    }

    pub async fn scan_range<R>(
        &self,
        txn: &R,
        prefix_values: &[NormalValue],
        lower: Bound,
        upper: Bound,
        reverse: bool,
    ) -> storage::corekv::Result<RangeIterator>
    where
        R: Reader + MaybeSend,
    {
        match &self.overlay {
            Some(overlay) => {
                self.index
                    .scan_range(
                        &OverlayReader::new(txn, overlay),
                        prefix_values,
                        lower,
                        upper,
                        reverse,
                    )
                    .await
            }
            None => {
                self.index
                    .scan_range(txn, prefix_values, lower, upper, reverse)
                    .await
            }
        }
    }
}
