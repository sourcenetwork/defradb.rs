use crate::database::DB;
use crate::error::{Error, Result};
use crate::txn::DbTxn;
use datastore::NamespaceView;
use query::fetcher::{DocumentArrival, DocumentArrivalOptions, DocumentArrivalPage};
use std::sync::Arc;
use storage::corekv::{IterOptions, Store};

fn head_key(collection: u32) -> Vec<u8> {
    format!("/arrival/{collection}/head").into_bytes()
}
fn row_key(collection: u32, cursor: u64) -> Vec<u8> {
    format!("/arrival/{collection}/row/{cursor:020}").into_bytes()
}
fn doc_key(collection: u32, doc: &str) -> Vec<u8> {
    format!("/arrival/{collection}/doc/{doc}").into_bytes()
}

async fn number(store: &NamespaceView, key: &[u8]) -> Result<u64> {
    match store.get(key).await {
        Ok(Some(bytes)) => {
            Ok(u64::from_be_bytes(bytes.as_ref().try_into().map_err(
                |_| Error::Serialization("invalid arrival cursor".into()),
            )?))
        }
        Ok(None) => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn pending_prefix(collection: u32) -> Vec<u8> {
    format!("{PENDING_ROOT}{collection}/").into_bytes()
}
fn pending_key(collection: u32, doc_short_id: u64) -> Vec<u8> {
    format!("{PENDING_ROOT}{collection}/{doc_short_id:020}").into_bytes()
}
const PENDING_ROOT: &str = "/arrival-pending/";

/// Journal `doc`'s arrival in the caller's transaction, durably and atomically
/// with the document. Each arrival writes only its own key, so overlapping
/// transactions never conflict on the journal; [`sequence`] numbers it once the
/// transaction commits. This journal starts at installation; existing documents
/// are deliberately not assigned fake history.
pub(crate) async fn record(
    store: &NamespaceView,
    collection: u32,
    doc_short_id: u64,
    doc: &str,
) -> Result<()> {
    if number(store, &doc_key(collection, doc)).await? == 0 {
        store
            .set(&pending_key(collection, doc_short_id), doc.as_bytes())
            .await?;
    }
    Ok(())
}

/// Assign cursors to `collection`'s committed arrivals, in short-ID order. Run
/// after every commit that recorded one.
///
/// Only this step writes the head, under the collection's arrival guard, so no
/// lower cursor can commit after a higher one. A run that loses a conflict to a
/// commit landing mid-scan leaves its arrivals pending: that commit's own run
/// follows under the guard and numbers them.
pub(crate) async fn sequence<S: Store>(db: &DB<S>, collection: u32) {
    let _guard = db.doc_write_queue().acquire_arrival(collection).await;
    match try_sequence(db, collection).await {
        Ok(()) => {}
        Err(error) if error.is_txn_conflict() => {
            tracing::debug!(collection, %error, "arrival sequencing left to the next run")
        }
        // Anything else repeats on every run, leaving arrivals pending until
        // the database reopens.
        Err(error) => tracing::warn!(collection, %error, "arrival sequencing failed"),
    }
}

/// Run [`sequence`] for `collection` once `txn` commits.
pub(crate) fn sequence_on_commit<S: Store + 'static>(
    txn: &mut DbTxn<S>,
    db: &Arc<DB<S>>,
    collection: u32,
) -> Result<()> {
    let db = db.clone();
    txn.on_success_async(Box::new(move || {
        Box::pin(async move { sequence(&db, collection).await })
    }))
}

/// Number every arrival a crash left pending.
pub(crate) async fn sequence_all<S: Store>(db: &DB<S>) -> Result<()> {
    let txn = db.new_txn(true).await?;
    let pending = pending(&txn.systemstore()?, PENDING_ROOT.as_bytes()).await?;
    txn.discard()?;
    let mut collections: Vec<u32> = pending
        .iter()
        .filter_map(|(key, _)| {
            std::str::from_utf8(key)
                .ok()?
                .split('/')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    collections.dedup();
    for collection in collections {
        sequence(db, collection).await;
    }
    Ok(())
}

async fn try_sequence<S: Store>(db: &DB<S>, collection: u32) -> Result<()> {
    let txn = db.new_txn(false).await?;
    {
        let store = txn.systemstore()?;
        let prefix = pending_prefix(collection);
        let pending = pending(&store, &prefix).await?;
        if pending.is_empty() {
            drop(store);
            return txn.discard();
        }
        let mut head = number(&store, &head_key(collection)).await?;
        for (suffix, doc) in &pending {
            let doc = std::str::from_utf8(doc)
                .map_err(|_| Error::Serialization("invalid pending arrival".into()))?;
            head = head
                .checked_add(1)
                .ok_or_else(|| Error::Serialization("arrival cursor exhausted".into()))?;
            store
                .set(&doc_key(collection, doc), &head.to_be_bytes())
                .await?;
            store
                .set(&row_key(collection, head), doc.as_bytes())
                .await?;
            store.delete(&[prefix.as_slice(), suffix].concat()).await?;
        }
        store
            .set(&head_key(collection), &head.to_be_bytes())
            .await?;
    }
    txn.commit().await
}

/// The `(key suffix, value)` pairs under `prefix`, in key order.
async fn pending(store: &NamespaceView, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut iter = store
        .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
        .await
        .map_err(Error::Storage)?;
    let mut entries = Vec::new();
    while let Some(kv) = iter.next().await.map_err(Error::Storage)? {
        entries.push((kv.key[prefix.len()..].to_vec(), kv.value.to_vec()));
    }
    Ok(entries)
}

pub(crate) async fn read<S: Store>(
    txn: &mut DbTxn<S>,
    options: &DocumentArrivalOptions,
) -> Result<DocumentArrivalPage> {
    let collection = txn
        .get_collection(&options.collection)
        .await?
        .ok_or_else(|| Error::CollectionNotFound(options.collection.clone()))?
        .resolved_root_id();
    let store = txn.systemstore()?;
    let head = number(&store, &head_key(collection)).await?;
    if options.after > head {
        return Err(Error::InvalidDocument(
            "arrival cursor is beyond this node's collection head".into(),
        ));
    }
    let limit = options.limit.min(1024);
    let mut entries = Vec::new();
    let next;
    if let Some(ids) = &options.doc_ids {
        for id in ids {
            let cursor = number(&store, &doc_key(collection, id)).await?;
            if cursor > options.after && cursor <= head {
                entries.push(DocumentArrival {
                    cursor,
                    doc_id: id.clone(),
                });
            }
        }
        entries.sort_by_key(|entry| entry.cursor);
        entries.dedup_by_key(|entry| entry.cursor);
        entries.truncate(limit as usize);
        next = entries.last().map_or(head, |entry| entry.cursor);
    } else {
        next = head.min(options.after.saturating_add(limit));
        for cursor in options.after.saturating_add(1)..=next {
            let bytes = store
                .get(&row_key(collection, cursor))
                .await?
                .ok_or_else(|| Error::Serialization("missing arrival row".into()))?;
            let doc_id = String::from_utf8(bytes.to_vec())
                .map_err(|e| Error::Serialization(e.to_string()))?;
            entries.push(DocumentArrival { cursor, doc_id });
        }
    }
    Ok(DocumentArrivalPage {
        head,
        next,
        entries,
    })
}

#[cfg(test)]
#[path = "../../tests/read/arrival_transactions.rs"]
mod tests;
