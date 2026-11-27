//! Finds a parent's chunk documents through the `_sourceID` index.

use document::NormalValue;
use query::error::{QueryError, Result};
use rapidhash::RapidHashSet;
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::Store;
use storage::index::{IndexIterator, RangeIterator};

use datastore::NamespaceView;

use crate::collection::loader::get_collection_with_index_manager;
use crate::collection::Collection;
use crate::index::IndexManager;
use crate::txn::DbTxn;

use super::{ChunkCollection, ChunkWrites};

/// About how many index entries a scan reads for the cost of one lookup.
const SCAN_ENTRIES_PER_LOOKUP: usize = 8;

impl<S> ChunkWrites<'_, S>
where
    S: Store + 'static,
{
    pub(super) async fn open(&self, name: &str) -> Result<ChunkCollection> {
        ChunkCollection::open(self.txn, name).await
    }
}

impl ChunkCollection {
    /// The chunk collection `name`, opened in `txn`.
    pub(crate) async fn open<S>(txn: &DbTxn<S>, name: &str) -> Result<Self>
    where
        S: Store + 'static,
    {
        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(txn, name).await?;
        Self::new(collection, datastore, systemstore, index_manager)
    }

    pub(crate) fn new(
        collection: Collection,
        datastore: NamespaceView,
        systemstore: NamespaceView,
        index_manager: IndexManager,
    ) -> Result<Self> {
        let index_name = collection
            .schema()
            .indexes
            .iter()
            .find(|index| {
                index
                    .fields
                    .first()
                    .is_some_and(|field| field.name == CHUNK_SOURCE_ID_FIELD)
            })
            .map(|index| index.name.clone())
            .ok_or_else(|| {
                QueryError::execution(format!(
                    "chunk collection {} has no source index",
                    collection.name()
                ))
            })?;
        Ok(Self {
            collection,
            datastore,
            systemstore,
            index_manager,
            index_name,
        })
    }

    /// Which of the `planned` hashes `parent` already has a chunk document for.
    ///
    /// A parent's old chunks stay until it is deleted, so its range of the
    /// index grows with every edit. It is scanned while that costs about what
    /// looking each planned hash up would; past that, the hashes the scan has
    /// not found are looked up one by one.
    pub(crate) async fn existing_of(
        &self,
        parent: &str,
        planned: &RapidHashSet<&str>,
    ) -> Result<RapidHashSet<String>> {
        let budget = planned.len().saturating_mul(SCAN_ENTRIES_PER_LOOKUP);
        let mut entries = self.scan_of(parent).await?;
        let mut existing = RapidHashSet::default();
        for _ in 0..budget {
            let Some(entry) = entries
                .next()
                .await
                .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?
            else {
                return Ok(existing);
            };
            // An index key does not record that the hash was a string.
            let hash = match entry.values.into_iter().nth(1) {
                Some(NormalValue::Bytes(hash)) => String::from_utf8(hash).ok(),
                Some(NormalValue::String(hash)) => Some(hash),
                _ => None,
            };
            if let Some(hash) = hash.filter(|hash| planned.contains(hash.as_str())) {
                existing.insert(hash);
            }
        }
        for hash in planned {
            if !existing.contains(*hash) && self.has(parent, hash).await? {
                existing.insert((*hash).to_string());
            }
        }
        Ok(existing)
    }

    async fn has(&self, parent: &str, hash: &str) -> Result<bool> {
        let mut entries = self.scan_of_hash(parent, hash).await?;
        let found = entries
            .next()
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?;
        Ok(found.is_some())
    }

    pub(super) async fn chunks_of(&self, parent: &str) -> Result<Vec<u64>> {
        short_ids(self.scan_of(parent).await?).await
    }

    /// The short IDs of `parent`'s chunks with `hash`. Peers that sign or
    /// encrypt their chunks can each hold one under its own ID.
    pub(crate) async fn chunks_with(&self, parent: &str, hash: &str) -> Result<Vec<u64>> {
        let entries = self.scan_of_hash(parent, hash).await?;
        short_ids(entries).await
    }

    async fn scan_of(&self, parent: &str) -> Result<RangeIterator> {
        self.scan(&[NormalValue::String(parent.to_string())]).await
    }

    async fn scan_of_hash(&self, parent: &str, hash: &str) -> Result<RangeIterator> {
        self.scan(&[
            NormalValue::String(parent.to_string()),
            NormalValue::String(hash.to_string()),
        ])
        .await
    }

    async fn scan(&self, prefix: &[NormalValue]) -> Result<RangeIterator> {
        let index = self
            .index_manager
            .get_index(&self.index_name)
            .ok_or_else(|| QueryError::execution("chunk source index is not loaded"))?;
        index
            .scan_prefix(&self.datastore, prefix, false)
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))
    }
}

async fn short_ids(mut entries: RangeIterator) -> Result<Vec<u64>> {
    let entries = entries
        .collect_all()
        .await
        .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?;
    Ok(entries
        .into_iter()
        .map(|entry| entry.doc_short_id)
        .collect())
}
