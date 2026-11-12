//! The per-collection journal of committed document updates, and its paged read.

mod gate;

use crate::error::{Error, Result};
use datastore::NamespaceView;
use query::fetcher::{DocumentReader, DocumentUpdate, DocumentUpdateOptions, DocumentUpdatePage};
use serde::{Deserialize, Serialize};
use storage::corekv::{AppendLog, IterOptions, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Update {
    pub doc_id: String,
    pub cid: String,
}

fn head_key(collection: u32) -> Vec<u8> {
    format!("/arrival/updates/{collection}/head").into_bytes()
}

fn row_prefix(collection: u32) -> Vec<u8> {
    format!("/arrival/updates/{collection}/row/").into_bytes()
}

fn row_key(collection: u32, cursor: u64) -> Vec<u8> {
    format!("/arrival/updates/{collection}/row/{cursor:020}").into_bytes()
}

fn once_key(collection: u32, doc: &str, cid: &str) -> Vec<u8> {
    let mut key = format!("/arrival/updates/{collection}/once/").into_bytes();
    key.extend_from_slice(&(doc.len() as u64).to_be_bytes());
    key.extend_from_slice(doc.as_bytes());
    key.extend_from_slice(cid.as_bytes());
    key
}

/// Attach an update to the document transaction; its cursor is assigned at commit.
pub(crate) async fn record(
    store: &NamespaceView,
    collection: u32,
    doc: &str,
    cid: &str,
) -> Result<()> {
    let row = serde_json::to_vec(&Update {
        doc_id: doc.into(),
        cid: cid.into(),
    })
    .map_err(|error| Error::Serialization(error.to_string()))?;
    store
        .append(
            &AppendLog::new(head_key(collection), row_prefix(collection)),
            &row,
            Some(&once_key(collection, doc, cid)),
        )
        .await
        .map_err(Error::Storage)
}

/// One page of the journal as `reader` may see it. The rows, their documents'
/// deletion state and the access decision are all read in `txn`.
pub(crate) async fn read<S>(
    txn: &crate::txn::DbTxn<S>,
    options: &DocumentUpdateOptions,
    reader: &DocumentReader<'_>,
) -> Result<DocumentUpdatePage>
where
    S: Store,
{
    let collection = txn
        .get_collection(&options.collection)
        .await?
        .ok_or_else(|| Error::CollectionNotFound(options.collection.clone()))?;
    let root = collection.resolved_root_id();
    let store = txn.systemstore()?;
    let head = match store.get(&head_key(root)).await? {
        Some(bytes) => u64::from_be_bytes(
            bytes
                .as_ref()
                .try_into()
                .map_err(|_| Error::Serialization("invalid update cursor".into()))?,
        ),
        None => 0,
    };
    if options.after > head {
        return Err(Error::InvalidDocument(
            "update cursor is beyond this node's collection head".into(),
        ));
    }
    let next = head.min(options.after.saturating_add(options.limit.min(1024)));
    let rows = rows(&store, root, options.after, next).await?;
    let mut gate = gate::Gate::new(txn, &collection, reader)?;
    let mut entries = Vec::with_capacity(rows.len());
    for (cursor, row) in rows {
        if gate.admits(&row.doc_id).await? {
            entries.push(DocumentUpdate {
                cursor,
                doc_id: row.doc_id,
                cid: row.cid,
            });
        }
    }
    Ok(DocumentUpdatePage {
        head,
        next,
        entries,
    })
}

/// [`read`] for a fetcher that scopes no provider to its transaction: the
/// reader's provider is bound to `txn` here, so it decides on what `txn`
/// reads. A request's own read transaction hands over one already bound.
pub(crate) async fn read_bound<S>(
    txn: &crate::txn::DbTxn<S>,
    options: &DocumentUpdateOptions,
    reader: &DocumentReader<'_>,
) -> Result<DocumentUpdatePage>
where
    S: Store,
{
    let bound = reader.acp.bind_transaction(txn.acpstore()?);
    read(
        txn,
        options,
        &DocumentReader {
            context: reader.context,
            acp: bound.as_deref().unwrap_or(reader.acp),
            projection: reader.projection,
            identity: reader.identity,
        },
    )
    .await
}

/// A journal read's error as its query reports it: an access decision's own
/// error stays what it was.
pub(crate) fn query_error(error: Error) -> query::error::QueryError {
    match error {
        Error::Query(error) => error,
        other => query::error::QueryError::execution(other.to_string()),
    }
}

/// The rows after `after` up to `next`, in one scan. Row keys are zero-padded,
/// so key order is cursor order, and a snapshot holds every row up to its
/// head: a key that is not the next cursor's means a row is missing.
async fn rows(
    store: &NamespaceView,
    collection: u32,
    after: u64,
    next: u64,
) -> Result<Vec<(u64, Update)>> {
    let mut rows = Vec::with_capacity((next - after) as usize);
    if after == next {
        return Ok(rows);
    }
    let mut scan = store
        .iterator(
            IterOptions::new()
                .with_prefix(row_prefix(collection))
                .with_start(row_key(collection, after + 1)),
        )
        .await?;
    for cursor in after + 1..=next {
        let pair = scan
            .next()
            .await?
            .filter(|pair| pair.key == row_key(collection, cursor))
            .ok_or_else(|| Error::Serialization("missing update row".into()))?;
        let row = serde_json::from_slice(&pair.value)
            .map_err(|error| Error::Serialization(error.to_string()))?;
        rows.push((cursor, row));
    }
    scan.close().await?;
    Ok(rows)
}

#[cfg(test)]
#[path = "../../tests/read/update_transactions.rs"]
mod tests;
