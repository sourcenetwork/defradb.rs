//! Exact match iterator for index lookups
//!
//! Provides iteration over index entries that exactly match specific field values.
//! Supports both unique and non-unique indexes.

use async_trait::async_trait;
use document::NormalValue;
use schema::IndexDescription;

use super::iterator::{IndexEntry, IndexIterator};
use super::range_iterator::RangeIterator;
use crate::corekv::{IterOptions, Iterator, MaybeSend, Reader, Result};
use crate::keys::datastore::IndexedField;
use crate::keys::doc_id_index::decode_doc_short_id;
use crate::keys::{IndexDataStoreKey, SEPARATOR};

/// Iterator for exact match queries on an index.
///
/// For SimpleIndex: scans all entries with the exact encoded value prefix,
/// extracting doc short IDs from the key suffix.
///
/// For UniqueIndex: performs a direct lookup for non-NULL values (single result,
/// doc short ID decoded from the value), or prefix scan for NULL values
/// (multiple results allowed, doc short ID in the key suffix).
pub struct ExactMatchIterator {
    /// The underlying KV iterator (None after exhaustion or for unique index after first result)
    inner: Option<Box<dyn Iterator>>,
    /// The key prefix (encoded field values without the doc short ID)
    key_prefix: Vec<u8>,
    /// The exact field values being matched
    values: Vec<NormalValue>,
    /// Single result for unique index direct lookup (consumed on first next())
    unique_result: Option<IndexEntry>,
    /// Cached copy of unique_result for reset() support
    cached_unique_result: Option<IndexEntry>,
    /// Whether the iterator has been exhausted
    exhausted: bool,
    /// Set when the values bind only the leading fields of a composite key.
    leading: Option<RangeIterator>,
}

impl ExactMatchIterator {
    /// Create a new exact match iterator for a simple (non-unique) index.
    ///
    /// Scans all index entries with the given field values.
    pub async fn new_simple<R: Reader + MaybeSend + ?Sized>(
        txn: &R,
        collection_short_id: u32,
        desc: &IndexDescription,
        values: &[NormalValue],
    ) -> Result<Self> {
        if values.len() < desc.fields.len() {
            return Self::new_leading(txn, collection_short_id, desc, false, values).await;
        }
        let fields = build_indexed_fields(values, desc);
        let key_prefix =
            IndexDataStoreKey::new(collection_short_id, desc.id, fields).try_bytes()?;

        let opts = IterOptions::default().with_prefix(key_prefix.clone());
        let inner = txn.iterator(opts).await?;

        Ok(Self {
            inner: Some(inner),
            key_prefix,
            values: values.to_vec(),
            unique_result: None,
            cached_unique_result: None,
            exhausted: false,
            leading: None,
        })
    }

    /// Create a new exact match iterator for a unique index.
    ///
    /// For non-NULL values: performs direct key lookup (single result).
    /// For NULL values: performs prefix scan (multiple results allowed).
    pub async fn new_unique<R: Reader + MaybeSend + ?Sized>(
        txn: &R,
        collection_short_id: u32,
        desc: &IndexDescription,
        values: &[NormalValue],
    ) -> Result<Self> {
        if values.len() < desc.fields.len() {
            return Self::new_leading(txn, collection_short_id, desc, true, values).await;
        }
        let fields = build_indexed_fields(values, desc);
        let key_prefix =
            IndexDataStoreKey::new(collection_short_id, desc.id, fields).try_bytes()?;

        // Check if any value is nil - if so, use prefix scan like SimpleIndex
        let has_nil = values.iter().any(|v| v.is_nil());

        if has_nil {
            // NULL values: use prefix scan (multiple documents can have NULL)
            let opts = IterOptions::default().with_prefix(key_prefix.clone());
            let inner = txn.iterator(opts).await?;

            Ok(Self {
                inner: Some(inner),
                key_prefix,
                values: values.to_vec(),
                unique_result: None,
                cached_unique_result: None,
                exhausted: false,
                leading: None,
            })
        } else {
            // Non-NULL values: direct lookup, doc short ID stored in the value
            let unique_result = if let Some(value_bytes) = txn.get(&key_prefix).await? {
                let doc_short_id = decode_doc_short_id(&value_bytes)?;
                Some(IndexEntry::new(doc_short_id, values.to_vec()))
            } else {
                None
            };

            Ok(Self {
                inner: None, // No iterator needed for direct lookup
                key_prefix,
                values: values.to_vec(),
                cached_unique_result: unique_result.clone(),
                unique_result,
                exhausted: false,
                leading: None,
            })
        }
    }

    /// The rest of the key follows the bound fields, so entries are decoded
    /// as whole composite keys, not read as a doc short ID right after them.
    async fn new_leading<R: Reader + MaybeSend + ?Sized>(
        txn: &R,
        collection_short_id: u32,
        desc: &IndexDescription,
        is_unique: bool,
        values: &[NormalValue],
    ) -> Result<Self> {
        let leading =
            RangeIterator::new_prefix(txn, collection_short_id, desc, is_unique, values, false)
                .await?;
        Ok(Self {
            inner: None,
            key_prefix: Vec::new(),
            values: values.to_vec(),
            unique_result: None,
            cached_unique_result: None,
            exhausted: false,
            leading: Some(leading),
        })
    }

    /// Extract the doc short ID from an index key.
    ///
    /// For entries carrying doc identity in the key, the suffix after the
    /// encoded field values is `/[uvarint doc short ID]`.
    fn extract_doc_short_id(&self, key: &[u8]) -> Result<u64> {
        let suffix = &key[self.key_prefix.len().min(key.len())..];
        if suffix.len() < 2 || suffix[0] != SEPARATOR {
            return Err(crate::corekv::Error::Other(
                "index key missing doc short ID suffix".to_string(),
            ));
        }
        decode_doc_short_id(&suffix[1..])
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IndexIterator for ExactMatchIterator {
    async fn next(&mut self) -> Result<Option<IndexEntry>> {
        if self.exhausted {
            return Ok(None);
        }

        if let Some(leading) = self.leading.as_mut() {
            let entry = leading.next().await?;
            self.exhausted = entry.is_none();
            return Ok(entry);
        }

        // Check for single unique result first
        if let Some(entry) = self.unique_result.take() {
            self.exhausted = true;
            return Ok(Some(entry));
        }

        // Use the inner iterator if available
        if let Some(ref mut iter) = self.inner {
            if let Some(kv) = iter.next().await? {
                // For unique index with NULL: doc short ID is in key suffix
                // For simple index: doc short ID is in key suffix
                // For unique index without NULL: handled above via unique_result
                let doc_short_id = self.extract_doc_short_id(&kv.key)?;
                return Ok(Some(IndexEntry::new(doc_short_id, self.values.clone())));
            }
        }

        self.exhausted = true;
        Ok(None)
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(ref mut iter) = self.inner {
            iter.close().await?;
        }
        if let Some(leading) = self.leading.as_mut() {
            leading.close().await?;
        }
        self.inner = None;
        self.exhausted = true;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        if let Some(ref mut iter) = self.inner {
            iter.reset().await?;
        }
        if let Some(leading) = self.leading.as_mut() {
            leading.reset().await?;
        }
        // Restore unique_result from cache for unique index direct lookups
        if self.cached_unique_result.is_some() && self.unique_result.is_none() {
            self.unique_result = self.cached_unique_result.clone();
        }
        self.exhausted = false;
        Ok(())
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        if self.exhausted {
            return Ok(false);
        }
        if let Some(ref mut iter) = self.inner {
            return iter.seek(key).await;
        }
        if let Some(leading) = self.leading.as_mut() {
            return leading.seek(key).await;
        }
        Ok(false)
    }
}

/// Build IndexedField structs from values and index description.
fn build_indexed_fields(values: &[NormalValue], desc: &IndexDescription) -> Vec<IndexedField> {
    values
        .iter()
        .zip(desc.fields.iter())
        .map(|(value, field_desc)| IndexedField::new(value.clone(), field_desc.descending))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::Store;
    use crate::index::{CollectionIndex, SimpleIndex, UniqueIndex};
    use schema::IndexedFieldDescription;

    fn test_index_description(unique: bool) -> IndexDescription {
        IndexDescription {
            id: 1,
            name: "test_index".to_string(),
            unique,
            kind: None,
            auto_generated: false,
            fields: vec![IndexedFieldDescription {
                name: "name".to_string(),
                descending: false,
            }],
        }
    }

    #[test]
    fn test_exact_match_simple_single_result() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();

            let desc = test_index_description(false);
            let index = SimpleIndex::new(1, desc.clone());

            index
                .save(&mut txn, 1, &[NormalValue::String("alice".to_string())])
                .await
                .unwrap();
            txn.commit().await.unwrap();

            let txn = store.new_txn(true).await.unwrap();
            let mut iter = ExactMatchIterator::new_simple(
                txn.as_ref(),
                1,
                &desc,
                &[NormalValue::String("alice".to_string())],
            )
            .await
            .unwrap();

            let entry = iter.next().await.unwrap();
            assert!(entry.is_some());
            let entry = entry.unwrap();
            assert_eq!(entry.doc_short_id, 1);

            let entry = iter.next().await.unwrap();
            assert!(entry.is_none());
        })
        .unwrap()
    }

    #[test]
    fn test_exact_match_simple_multiple_results() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();

            let desc = test_index_description(false);
            let index = SimpleIndex::new(1, desc.clone());

            // Same value, multiple documents
            index
                .save(&mut txn, 1, &[NormalValue::String("alice".to_string())])
                .await
                .unwrap();
            index
                .save(&mut txn, 2, &[NormalValue::String("alice".to_string())])
                .await
                .unwrap();
            index
                .save(&mut txn, 3, &[NormalValue::String("bob".to_string())])
                .await
                .unwrap();
            txn.commit().await.unwrap();

            let txn = store.new_txn(true).await.unwrap();
            let mut iter = ExactMatchIterator::new_simple(
                txn.as_ref(),
                1,
                &desc,
                &[NormalValue::String("alice".to_string())],
            )
            .await
            .unwrap();

            let entries = iter.collect_all().await.unwrap();
            assert_eq!(entries.len(), 2);
            assert!(entries.iter().any(|e| e.doc_short_id == 1));
            assert!(entries.iter().any(|e| e.doc_short_id == 2));
        })
        .unwrap()
    }

    #[test]
    fn test_exact_match_unique_single_result() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();

            let desc = test_index_description(true);
            let index = UniqueIndex::new(1, desc.clone());

            index
                .save(&mut txn, 1, &[NormalValue::String("alice".to_string())])
                .await
                .unwrap();
            txn.commit().await.unwrap();

            let txn = store.new_txn(true).await.unwrap();
            let mut iter = ExactMatchIterator::new_unique(
                txn.as_ref(),
                1,
                &desc,
                &[NormalValue::String("alice".to_string())],
            )
            .await
            .unwrap();

            let entry = iter.next().await.unwrap();
            assert!(entry.is_some());
            let entry = entry.unwrap();
            assert_eq!(entry.doc_short_id, 1);

            let entry = iter.next().await.unwrap();
            assert!(entry.is_none());
        })
        .unwrap()
    }

    #[test]
    fn test_exact_match_unique_not_found() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();

            let desc = test_index_description(true);
            let index = UniqueIndex::new(1, desc.clone());

            index
                .save(&mut txn, 1, &[NormalValue::String("alice".to_string())])
                .await
                .unwrap();
            txn.commit().await.unwrap();

            let txn = store.new_txn(true).await.unwrap();
            let mut iter = ExactMatchIterator::new_unique(
                txn.as_ref(),
                1,
                &desc,
                &[NormalValue::String("bob".to_string())],
            )
            .await
            .unwrap();

            let entry = iter.next().await.unwrap();
            assert!(entry.is_none());
        })
        .unwrap()
    }

    #[test]
    fn test_exact_match_unique_null_multiple() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();

            let desc = test_index_description(true);
            let index = UniqueIndex::new(1, desc.clone());

            // Multiple documents with NULL - allowed for unique index
            index.save(&mut txn, 1, &[NormalValue::Null]).await.unwrap();
            index.save(&mut txn, 2, &[NormalValue::Null]).await.unwrap();
            txn.commit().await.unwrap();

            let txn = store.new_txn(true).await.unwrap();
            let mut iter =
                ExactMatchIterator::new_unique(txn.as_ref(), 1, &desc, &[NormalValue::Null])
                    .await
                    .unwrap();

            let entries = iter.collect_all().await.unwrap();
            assert_eq!(entries.len(), 2);
        })
        .unwrap()
    }

    fn composite_description(unique: bool, descending: bool) -> IndexDescription {
        let field = |name: &str| IndexedFieldDescription {
            name: name.to_string(),
            descending,
        };
        IndexDescription {
            id: 1,
            name: "author_title".to_string(),
            unique,
            kind: None,
            auto_generated: false,
            fields: vec![field("author"), field("title")],
        }
    }

    fn pair(author: &str, title: &str) -> [NormalValue; 2] {
        [
            NormalValue::String(author.to_string()),
            NormalValue::String(title.to_string()),
        ]
    }

    async fn leading_lookup(unique: bool, descending: bool) -> Vec<u64> {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        let desc = composite_description(unique, descending);
        let rows = [
            (1, pair("a", "x")),
            (2, pair("b", "y")),
            (3, pair("a", "w")),
        ];
        for (short_id, values) in &rows {
            if unique {
                UniqueIndex::new(1, desc.clone())
                    .save(&mut txn, *short_id, values)
                    .await
                    .unwrap();
            } else {
                SimpleIndex::new(1, desc.clone())
                    .save(&mut txn, *short_id, values)
                    .await
                    .unwrap();
            }
        }
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        let lead = [NormalValue::String("a".to_string())];
        let mut iter = if unique {
            ExactMatchIterator::new_unique(txn.as_ref(), 1, &desc, &lead).await
        } else {
            ExactMatchIterator::new_simple(txn.as_ref(), 1, &desc, &lead).await
        }
        .unwrap();
        let entries = iter.collect_all().await.unwrap();
        assert!(entries.iter().all(|entry| entry.values.len() == 2));
        entries
            .into_iter()
            .map(|entry| entry.doc_short_id)
            .collect()
    }

    #[test]
    fn a_leading_value_of_a_composite_index_finds_its_documents_in_key_order() {
        workers::block_on(async {
            assert_eq!(leading_lookup(false, false).await, [3, 1]);
            assert_eq!(leading_lookup(false, true).await, [1, 3]);
            assert_eq!(leading_lookup(true, false).await, [3, 1]);
            assert_eq!(leading_lookup(true, true).await, [1, 3]);
        })
        .unwrap()
    }
}
