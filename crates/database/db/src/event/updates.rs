//! The per-collection journal of committed document updates, and its paged read.

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

pub(super) fn row_prefix(collection: u32) -> Vec<u8> {
    format!("/arrival/updates/{collection}/row/").into_bytes()
}

fn row_key(collection: u32, cursor: u64) -> Vec<u8> {
    format!("/arrival/updates/{collection}/row/{cursor:020}").into_bytes()
}

pub(super) fn log(collection: u32) -> AppendLog {
    AppendLog::new(head_key(collection), row_prefix(collection))
}

/// The head of `collection`'s journal: the cursor of its newest row, 0 for none.
pub(super) async fn stored_head(store: &NamespaceView, collection: u32) -> Result<u64> {
    match store.get(&head_key(collection)).await? {
        Some(bytes) => {
            Ok(u64::from_be_bytes(bytes.as_ref().try_into().map_err(
                |_| Error::Serialization("invalid update cursor".into()),
            )?))
        }
        None => Ok(0),
    }
}

/// The key that lets one block of one document add one row. It names the
/// document by its short ID, not its DocID: a full truncate drops the rows but
/// cannot drop their once keys (regolith writes them, and only `append` may),
/// and a document re-created after it takes a new short ID, so its updates
/// add rows again even when they repeat a dropped row's DocID and CID.
fn once_key(collection: u32, doc_short_id: u64, cid: &str) -> Vec<u8> {
    let mut key = format!("/arrival/updates/{collection}/once/").into_bytes();
    key.extend_from_slice(&doc_short_id.to_be_bytes());
    key.extend_from_slice(cid.as_bytes());
    key
}

/// Attach an update to the document transaction; its cursor is assigned at commit.
pub(crate) async fn record(
    store: &NamespaceView,
    collection: u32,
    doc: &str,
    doc_short_id: u64,
    cid: &str,
) -> Result<()> {
    let row = serde_json::to_vec(&Update {
        doc_id: doc.into(),
        cid: cid.into(),
    })
    .map_err(|error| Error::Serialization(error.to_string()))?;
    store
        .append(
            &log(collection),
            &row,
            Some(&once_key(collection, doc_short_id, cid)),
        )
        .await
        .map_err(Error::Storage)
}

/// One page of the journal as `reader` may see it.
///
/// `txn` reads the rows and their documents' deletion state. The reader's
/// provider is bound to `txn`, so it decides on what `txn` reads. A provider
/// that cannot be bound decides outside `txn`, on live state; what it admits
/// is then confirmed alive in a snapshot taken after it decided, of `db` or
/// of the store `txn` was begun on.
pub(crate) async fn read<S>(
    db: Option<&crate::database::DB<S>>,
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
    let head = stored_head(&store, root).await?;
    if options.after > head {
        return Err(Error::InvalidDocument(
            "update cursor is beyond this node's collection head".into(),
        ));
    }
    let next = head.min(options.after.saturating_add(options.limit.min(1024)));
    let rows = rows(&store, root, options.after, next).await?;

    let bound = reader.acp.bind_transaction(txn.acpstore()?);
    let reader = DocumentReader {
        context: reader.context,
        acp: bound.as_deref().unwrap_or(reader.acp),
        projection: reader.projection,
        identity: reader.identity,
    };
    let mut gate = super::gate::Gate::new(txn, &collection, &reader)?;
    for (_, row) in &rows {
        gate.decide(&row.doc_id).await?;
    }
    if bound.is_none() {
        gate.confirm(db, txn).await?;
    }
    let entries = rows
        .into_iter()
        .filter(|(_, row)| gate.admits(&row.doc_id))
        .map(|(cursor, row)| DocumentUpdate {
            cursor,
            doc_id: row.doc_id,
            cid: row.cid,
        })
        .collect();
    Ok(DocumentUpdatePage {
        head,
        next,
        entries,
    })
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
/// so key order is cursor order, and a snapshot holds every row from the
/// oldest one kept up to its head: a first row that is not there was dropped
/// (`update_retention`), and any later key that is not the next cursor's means
/// a row is missing.
pub(super) async fn rows(
    store: &NamespaceView,
    collection: u32,
    after: u64,
    next: u64,
) -> Result<Vec<(u64, Update)>> {
    if after == next {
        return Ok(Vec::new());
    }
    let scan = store
        .iterator(
            IterOptions::new()
                .with_prefix(row_prefix(collection))
                .with_start(row_key(collection, after + 1)),
        )
        .await?;
    match drain(scan, collection, after, next).await? {
        Some(rows) => Ok(rows),
        None => Err(super::update_retention::missing_row(store, collection, after).await),
    }
}

/// Read the page from `scan` and close it, whether or not every row was there;
/// `None` when the first row is not there.
async fn drain(
    mut scan: Box<dyn storage::corekv::Iterator>,
    collection: u32,
    after: u64,
    next: u64,
) -> Result<Option<Vec<(u64, Update)>>> {
    let rows = scanned(scan.as_mut(), collection, after, next).await;
    let closed = scan.close().await;
    let rows = rows?;
    closed?;
    Ok(rows)
}

async fn scanned(
    scan: &mut dyn storage::corekv::Iterator,
    collection: u32,
    after: u64,
    next: u64,
) -> Result<Option<Vec<(u64, Update)>>> {
    let mut rows = Vec::with_capacity((next - after) as usize);
    for cursor in after + 1..=next {
        let pair = scan
            .next()
            .await?
            .filter(|pair| pair.key == row_key(collection, cursor));
        let Some(pair) = pair else {
            if cursor == after + 1 {
                return Ok(None);
            }
            return Err(Error::Serialization("missing update row".into()));
        };
        let row = serde_json::from_slice(&pair.value)
            .map_err(|error| Error::Serialization(error.to_string()))?;
        rows.push((cursor, row));
    }
    Ok(Some(rows))
}

#[cfg(test)]
#[path = "../../tests/read/update_transactions.rs"]
mod tests;
