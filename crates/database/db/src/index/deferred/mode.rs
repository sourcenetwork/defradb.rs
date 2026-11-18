//! Whether an index's stored state belongs to the other way of keeping it,
//! after the types registered at a start changed whether it is deferred
//! (`proofs/tla/DeferredIndexMode.tla`).

use datastore::NamespaceView;
use storage::corekv::IterOptions;
use storage::keys::{index_pending_log, index_position_prefix, IndexDataStoreKey};

use crate::index::error::{Error, Result};

/// A deferred index with entries and not one position record was kept
/// inline; an inline index with records or a log was kept deferred.
pub(crate) async fn kept_in_other_mode(
    datastore: &NamespaceView,
    collection_short_id: u32,
    index_id: u32,
    deferred: bool,
) -> Result<bool> {
    let records = any(
        datastore,
        index_position_prefix(collection_short_id, index_id),
    );
    if deferred {
        return Ok(any(
            datastore,
            IndexDataStoreKey::index_prefix(collection_short_id, index_id),
        )
        .await?
            && !records.await?);
    }
    Ok(records.await?
        || any(
            datastore,
            index_pending_log(collection_short_id, index_id)
                .entry_prefix()
                .to_vec(),
        )
        .await?)
}

async fn any(datastore: &NamespaceView, prefix: Vec<u8>) -> Result<bool> {
    let mut iter = datastore
        .iterator(
            IterOptions::default()
                .with_prefix(prefix)
                .with_keys_only(true),
        )
        .await
        .map_err(Error::Storage)?;
    let found = iter.next().await.map_err(Error::Storage)?.is_some();
    iter.close().await.map_err(Error::Storage)?;
    Ok(found)
}
