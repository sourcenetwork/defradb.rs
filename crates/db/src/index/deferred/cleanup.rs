//! Removing what a dropped deferred index keeps besides its entries.

use datastore::NamespaceView;
use storage::corekv::IterOptions;
use storage::keys::{index_pending_log, index_position_prefix};

use crate::index::error::{Error, Result};

/// Removes up to `limit` position records of the index after `after`, and the
/// whole pending log once none is left. Returns how many records it removed
/// and the last of them, as `CollectionIndex::remove_page` does.
pub(crate) async fn remove_page(
    datastore: &NamespaceView,
    collection_short_id: u32,
    index_id: u32,
    after: Option<&[u8]>,
    limit: usize,
) -> Result<(usize, Option<Vec<u8>>)> {
    let limit = limit.max(1);
    let mut options = IterOptions::default()
        .with_prefix(index_position_prefix(collection_short_id, index_id))
        .with_keys_only(true);
    if let Some(after) = after {
        let mut start = after.to_vec();
        start.push(0);
        options = options.with_start(start);
    }
    let mut keys = Vec::new();
    let mut iter = datastore.iterator(options).await.map_err(Error::Storage)?;
    while keys.len() < limit {
        let Some(pair) = iter.next().await.map_err(Error::Storage)? else {
            break;
        };
        keys.push(pair.key);
    }
    iter.close().await.map_err(Error::Storage)?;
    for key in &keys {
        datastore.delete(key).await.map_err(Error::Storage)?;
    }
    if keys.len() < limit {
        let log = index_pending_log(collection_short_id, index_id);
        let head = datastore
            .get(log.head_key())
            .await
            .map_err(Error::Storage)?
            .and_then(|bytes| <[u8; 8]>::try_from(bytes.as_ref()).ok())
            .map_or(0, u64::from_be_bytes);
        if head > 0 {
            datastore
                .truncate_log(&log, 1..=head)
                .await
                .map_err(Error::Storage)?;
        }
    }
    Ok((keys.len(), keys.pop()))
}
