//! How long the update journal keeps its rows, and what a reader is told
//! about a cursor whose rows are gone.
//!
//! A row stays until its collection is truncated in full. That truncate drops
//! the rows from the oldest one still kept up to the head it read, so the rows
//! a snapshot holds are always one run from the oldest kept row to the head,
//! and the head keeps counting: no cursor is handed out twice
//! (`proofs/tla/UpdateJournalRetention.tla`).

use std::sync::Arc;

use datastore::NamespaceView;
use storage::corekv::{IterOptions, Store};

use super::updates::{log, row_prefix, stored_head};
use crate::error::{Error, Result};

/// The most rows one transaction drops: each costs one buffered delete.
const SLICE: u64 = 1024;

/// Drop every row of `collection`'s journal up to the head it has now, in
/// slices of at most [`SLICE`] rows, each committed before the next begins.
///
/// Every slice starts at the oldest row still kept, so a crash or a concurrent
/// truncate leaves the dropped rows a prefix, and a slice run twice changes
/// nothing.
pub(crate) async fn drop_rows<S>(db: &Arc<crate::database::DB<S>>, collection: u32) -> Result<()>
where
    S: Store + 'static,
{
    let through = db
        .transact_db(move |txn| {
            Box::pin(async move { stored_head(&txn.systemstore()?, collection).await })
        })
        .await?;
    loop {
        let dropped = db
            .transact_db(move |txn| {
                Box::pin(async move {
                    let store = txn.systemstore()?;
                    let Some(first) = oldest_row(&store, collection).await? else {
                        return Ok(false);
                    };
                    if first > through {
                        return Ok(false);
                    }
                    let last = through.min(first + SLICE - 1);
                    store
                        .truncate_log(&log(collection), first..=last)
                        .await
                        .map_err(Error::Storage)?;
                    Ok(true)
                })
            })
            .await?;
        if !dropped {
            return Ok(());
        }
    }
}

/// The cursor of the oldest row `store` still holds for `collection`, if any.
pub(super) async fn oldest_row(store: &NamespaceView, collection: u32) -> Result<Option<u64>> {
    let prefix = row_prefix(collection);
    let mut scan = store
        .iterator(
            IterOptions::new()
                .with_prefix(prefix.clone())
                .with_keys_only(true),
        )
        .await?;
    let first = scan.next().await;
    let closed = scan.close().await;
    let first = first?;
    closed?;
    first
        .map(|pair| {
            std::str::from_utf8(&pair.key[prefix.len()..])
                .ok()
                .and_then(|cursor| cursor.parse().ok())
                .ok_or_else(|| Error::Serialization("invalid update row key".into()))
        })
        .transpose()
}

/// Why the row after `after` is not in `store`: its rows were dropped, when
/// no row at or before it is kept and the head is past it, or else a missing
/// row.
pub(super) async fn missing_row(store: &NamespaceView, collection: u32, after: u64) -> Error {
    let oldest = match oldest_row(store, collection).await {
        Ok(Some(first)) if first > after + 1 => first - 1,
        Ok(Some(_)) => return Error::Serialization("missing update row".into()),
        Ok(None) => match stored_head(store, collection).await {
            Ok(head) if head > after => head,
            Ok(_) => return Error::Serialization("missing update row".into()),
            Err(error) => return error,
        },
        Err(error) => return error,
    };
    Error::InvalidDocument(format!(
        "update cursor is no longer available on this node: resync the collection and read after cursor {oldest}"
    ))
}

#[cfg(test)]
#[path = "../../tests/read/update_retention.rs"]
mod tests;
