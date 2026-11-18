//! CollectionIndex trait definition

use async_trait::async_trait;
use document::NormalValue;
use schema::IndexDescription;

use crate::corekv::{IterOptions, MaybeSend, MaybeSendSync, Reader, Result, Writer};

/// Trait for collection index implementations.
///
/// Indexes maintain secondary lookup structures for efficient querying
/// of documents by field values other than the primary key.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CollectionIndex: MaybeSendSync {
    /// Returns the index description (metadata).
    fn description(&self) -> &IndexDescription;

    /// Save adds a new document to the index.
    ///
    /// Called when a new document is created in the collection.
    /// The values slice contains the field values in index field order.
    async fn save<T: Reader + Writer + MaybeSend>(
        &self,
        txn: &mut T,
        doc_short_id: u64,
        values: &[NormalValue],
    ) -> Result<()>;

    /// Update modifies an existing document's index entry.
    ///
    /// Called when a document is updated. Removes the old entry
    /// and adds a new one with the updated values.
    async fn update<T: Reader + Writer + MaybeSend>(
        &self,
        txn: &mut T,
        doc_short_id: u64,
        old_values: &[NormalValue],
        new_values: &[NormalValue],
    ) -> Result<()>;

    /// Delete removes a document from the index.
    ///
    /// Called when a document is deleted from the collection.
    async fn delete<T: Reader + Writer + MaybeSend>(
        &self,
        txn: &mut T,
        doc_short_id: u64,
        values: &[NormalValue],
    ) -> Result<()>;

    /// RemoveAll removes all entries for this index.
    ///
    /// Called when the index is dropped from the collection.
    async fn remove_all<T: Reader + Writer + MaybeSend>(&self, txn: &mut T) -> Result<()>;

    /// The prefix every key this index stores starts with.
    fn key_prefix(&self) -> Vec<u8>;

    /// Removes this index's first `limit` keys (at least one) in key order
    /// after `after`, or from its start when `after` is `None`. Returns how
    /// many it removed and the last of them, so a caller clears a large
    /// index in bounded transactions by passing that key back; fewer than
    /// `limit` means none is left after `after`.
    async fn remove_page<T>(
        &self,
        txn: &mut T,
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<(usize, Option<Vec<u8>>)>
    where
        T: Reader + Writer + MaybeSend,
    {
        let limit = limit.max(1);
        let mut options = IterOptions::default()
            .with_prefix(self.key_prefix())
            .with_keys_only(true);
        if let Some(after) = after {
            let mut start = after.to_vec();
            start.push(0);
            options = options.with_start(start);
        }
        let mut keys = Vec::new();
        {
            let mut iter = txn.iterator(options).await?;
            while keys.len() < limit {
                let Some(pair) = iter.next().await? else {
                    break;
                };
                keys.push(pair.key);
            }
            iter.close().await?;
        }
        for key in &keys {
            txn.delete(key).await?;
        }
        Ok((keys.len(), keys.pop()))
    }
}
