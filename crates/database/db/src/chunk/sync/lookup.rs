//! Finds a parent's chunk documents through the `_sourceID` index.

use document::NormalValue;
use query::error::{QueryError, Result};
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::Store;
use storage::index::{IndexIterator, RangeIterator};

use datastore::NamespaceView;

use crate::collection::loader::get_collection_with_index_manager;
use crate::collection::Collection;
use crate::index::IndexManager;
use crate::txn::DbTxn;

use super::{ChunkCollection, ChunkWrites};

/// A stored chunk of a parent: its short ID and the commit it names.
pub(crate) struct Stored {
    pub(crate) short_id: u64,
    pub(crate) source: Option<String>,
}

/// What [`ChunkCollection::settle`] decides.
#[derive(Default)]
pub(crate) struct Settled {
    /// The chunks to delete.
    pub(crate) delete: Vec<u64>,
    /// The hashes that keep a chunk, so need none made.
    pub(crate) held: RapidHashSet<String>,
}

impl<S> ChunkWrites<'_, S>
where
    S: Store + 'static,
{
    /// Whether the parent's current blocks encrypt `field`, by document or by field.
    pub(super) async fn parent_encrypts(&self, parent_doc_id: &str, field: &str) -> Result<bool> {
        let systemstore = self
            .txn
            .systemstore()
            .map_err(|e| QueryError::execution(e.to_string()))?;
        let Some(parent) = crate::docid::map::get_doc_ref(&systemstore, parent_doc_id)
            .await
            .map_err(|e| QueryError::execution(e.to_string()))?
        else {
            return Ok(false);
        };
        let (encrypt_doc, fields) = crate::block::builder::inherited_encryption_policy(
            &self.blockstore()?,
            &self.headstore()?,
            parent,
            &[field.to_string()],
        )
        .await
        .map_err(QueryError::execution)?;
        Ok(encrypt_doc || !fields.is_empty())
    }

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

    /// The live chunk documents of `parent`, by hash. Validated: what it finds decides writes.
    pub(crate) async fn stored_of(
        &self,
        parent: &str,
    ) -> Result<RapidHashMap<String, Vec<Stored>>> {
        let index = self.index()?;
        let mut entries = index
            .scan_prefix_validated(&self.datastore, &[NormalValue::String(parent.to_string())])
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?;
        let mut stored: RapidHashMap<String, Vec<Stored>> = RapidHashMap::default();
        while let Some(entry) = entries
            .next()
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?
        {
            let mut values = entry.values.into_iter().skip(1);
            if let Some(hash) = values.next().and_then(text) {
                stored.entry(hash).or_default().push(Stored {
                    short_id: entry.doc_short_id,
                    source: values.next().and_then(text),
                });
            }
        }
        Ok(stored)
    }

    /// Whether a chunk naming `source` waits for that commit, so nothing here may delete it.
    pub(crate) async fn waits(
        &self,
        parent: &str,
        source: Option<&str>,
        merging: Option<&cid::Cid>,
    ) -> Result<bool> {
        let Some(source) = source else {
            return Ok(false);
        };
        Ok(
            !crate::chunk::source::is_merged(&self.systemstore, parent, source, merging)
                .await
                .map_err(|e| QueryError::execution(e.to_string()))?,
        )
    }

    /// The chunks to delete, and the hashes still held, when the parent's field has one head.
    pub(crate) async fn settle(
        &self,
        parent: &str,
        used: &RapidHashSet<&str>,
        stored: &RapidHashMap<String, Vec<Stored>>,
        merging: Option<&cid::Cid>,
    ) -> Result<Settled> {
        let mut settled = Settled::default();
        for (hash, docs) in stored {
            let used = used.contains(hash.as_str());
            // A merge leaves every used hash a chunk, so a lone one is that one.
            if used && docs.len() == 1 {
                settled.held.insert(hash.clone());
                continue;
            }
            let mut ready = Vec::new();
            for doc in docs {
                if !self.waits(parent, doc.source.as_deref(), merging).await? {
                    ready.push((self.doc_id(doc.short_id).await?, doc.short_id));
                }
            }
            // Keep the smallest DocID, which every node compares the same way.
            if used && !ready.is_empty() {
                ready.sort_unstable();
                ready.remove(0);
                settled.held.insert(hash.clone());
            }
            settled
                .delete
                .extend(ready.into_iter().map(|(_, short_id)| short_id));
        }
        Ok(settled)
    }

    /// The DocID of the chunk `doc_short_id`, empty when it has none.
    pub(crate) async fn doc_id(&self, doc_short_id: u64) -> Result<String> {
        Ok(
            crate::docid::map::get_doc_id(&self.systemstore, doc_short_id)
                .await
                .map_err(|e| QueryError::execution(e.to_string()))?
                .unwrap_or_default(),
        )
    }

    // Validated, since a parent's delete deletes what it finds.
    pub(super) async fn chunks_of(&self, parent: &str) -> Result<Vec<u64>> {
        let entries = self
            .index()?
            .scan_prefix_validated(&self.datastore, &[NormalValue::String(parent.to_string())])
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?;
        short_ids(entries).await
    }

    /// The short IDs of `parent`'s chunks with `hash`; an encrypted one can have a copy per node.
    pub(crate) async fn chunks_with(&self, parent: &str, hash: &str) -> Result<Vec<u64>> {
        let entries = self
            .scan(&[
                NormalValue::String(parent.to_string()),
                NormalValue::String(hash.to_string()),
            ])
            .await?;
        short_ids(entries).await
    }

    fn index(&self) -> Result<&crate::index::manager::IndexType> {
        self.index_manager
            .get_index(&self.index_name)
            .ok_or_else(|| QueryError::execution("chunk source index is not loaded"))
    }

    async fn scan(&self, prefix: &[NormalValue]) -> Result<RangeIterator> {
        self.index()?
            .scan_prefix(&self.datastore, prefix, false)
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))
    }
}

// An index key does not record that a value was a string.
fn text(value: NormalValue) -> Option<String> {
    match value {
        NormalValue::Bytes(bytes) => String::from_utf8(bytes).ok(),
        NormalValue::String(text) => Some(text),
        _ => None,
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
