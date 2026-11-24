//! Removing what a dropped deferred index keeps besides its entries.

use datastore::NamespaceView;
use storage::corekv::IterOptions;
use storage::keys::{index_pending_log, index_position_prefix};

use super::drain::position;
use crate::index::error::{Error, Result};

/// Whether `key` is past the entries of the index: one of its records or
/// log entries, where a drop's cleanup carries on from.
pub(crate) fn past_entries(collection_short_id: u32, index_id: u32, key: &[u8]) -> bool {
    key.starts_with(&index_position_prefix(collection_short_id, index_id))
        || key.starts_with(index_pending_log(collection_short_id, index_id).entry_prefix())
}

/// Removes up to `limit` position records of the index after `after`, then,
/// once none is left, up to `limit` log entries. Returns how many it removed
/// in the last step and the last key, as `CollectionIndex::remove_page` does.
pub(crate) async fn remove_page(
    datastore: &NamespaceView,
    collection_short_id: u32,
    index_id: u32,
    after: Option<&[u8]>,
    limit: usize,
) -> Result<(usize, Option<Vec<u8>>)> {
    let limit = limit.max(1);
    let log = index_pending_log(collection_short_id, index_id);
    let in_log = after.is_some_and(|after| after.starts_with(log.entry_prefix()));
    let mut last = None;
    if !in_log {
        let records = page(
            datastore,
            index_position_prefix(collection_short_id, index_id),
            after,
            limit,
        )
        .await?;
        for key in &records {
            datastore.delete(key).await.map_err(Error::Storage)?;
        }
        if records.len() == limit {
            return Ok((records.len(), records.last().cloned()));
        }
        last = records.last().cloned();
    }
    let entries = page(
        datastore,
        log.entry_prefix().to_vec(),
        after.filter(|_| in_log),
        limit,
    )
    .await?;
    // Only the positions still there: each one in a range costs a delete.
    let mut runs: Vec<(u64, u64)> = Vec::new();
    for key in &entries {
        let at = position(key)?;
        match runs.last_mut() {
            Some((_, through)) if *through + 1 == at => *through = at,
            _ => runs.push((at, at)),
        }
    }
    for (first, through) in runs {
        datastore
            .truncate_log(&log, first..=through)
            .await
            .map_err(Error::Storage)?;
    }
    Ok((entries.len(), entries.last().cloned().or(last)))
}

/// Up to `limit` keys under `prefix` after `after`.
async fn page(
    datastore: &NamespaceView,
    prefix: Vec<u8>,
    after: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<Vec<u8>>> {
    let mut options = IterOptions::default()
        .with_prefix(prefix)
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
    Ok(keys)
}
