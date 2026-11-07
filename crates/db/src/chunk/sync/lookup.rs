//! Finds a parent's chunk documents through the `_sourceID` index.

use document::NormalValue;
use query::error::{QueryError, Result};
use rapidhash::RapidHashSet;
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::Store;
use storage::index::{IndexIterator, RangeIterator};

use crate::collection::loader::get_collection_with_index_manager;

use super::{ChunkCollection, ChunkWrites};

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
        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(self.txn, name).await?;
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
                QueryError::execution(format!("chunk collection {name} has no source index"))
            })?;
        Ok(ChunkCollection {
            collection,
            datastore,
            systemstore,
            index_manager,
            index_name,
        })
    }

    /// Which of the `planned` hashes `parent` already has a chunk document for.
    /// Only those are kept, since a parent's old chunks stay until it is deleted.
    pub(super) async fn existing_of(
        &self,
        target: &ChunkCollection,
        parent: &str,
        planned: &RapidHashSet<&str>,
    ) -> Result<RapidHashSet<String>> {
        let mut entries = self.scan_of(target, parent).await?;
        let mut existing = RapidHashSet::default();
        while let Some(entry) = entries
            .next()
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?
        {
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
        Ok(existing)
    }

    pub(super) async fn chunks_of(
        &self,
        target: &ChunkCollection,
        parent: &str,
    ) -> Result<Vec<u64>> {
        let entries = self
            .scan_of(target, parent)
            .await?
            .collect_all()
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))?;
        Ok(entries
            .into_iter()
            .map(|entry| entry.doc_short_id)
            .collect())
    }

    async fn scan_of(&self, target: &ChunkCollection, parent: &str) -> Result<RangeIterator> {
        let index = target
            .index_manager
            .get_index(&target.index_name)
            .ok_or_else(|| QueryError::execution("chunk source index is not loaded"))?;
        index
            .scan_prefix(
                &target.datastore,
                &[NormalValue::String(parent.to_string())],
                false,
            )
            .await
            .map_err(|e| QueryError::execution(format!("chunk scan failed: {e}")))
    }
}
