use crate::error::{Error, Result};
use crate::txn::DbTxn;
use datastore::NamespaceView;
use query::fetcher::{DocumentArrival, DocumentArrivalOptions, DocumentArrivalPage};
use storage::corekv::Store;

// DEFRALEVEL(S6): Remove head from commit path; sequencer manages cursor high-water.
fn head_key(collection: u32) -> Vec<u8> {
    format!("/arrival/{collection}/head").into_bytes()
}
// DEFRALEVEL(S6): Paginated index written by sequencer post-commit; rows gap-free.
fn row_key(collection: u32, cursor: u64) -> Vec<u8> {
    format!("/arrival/{collection}/row/{cursor:020}").into_bytes()
}
// DEFRALEVEL(S6): Keep as doc->cursor index the sequencer writes post-commit; the in-txn blind pending marker gets its own per-doc key
fn doc_key(collection: u32, doc: &str) -> Vec<u8> {
    format!("/arrival/{collection}/doc/{doc}").into_bytes()
}

// DEFRALEVEL(S6): DefraLevel validates head reads; reused by sequencer and read().
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

// DEFRALEVEL(S6): Rewrite invariant; moves from OCC on head to sequencing.
// DEFRALEVEL(S6): Blind per-doc pending marker as merge operand; /arrival/ is Ordinary, puts conflict; sequence post-commit
/// The head and arrival row share the document transaction. The shared head
/// key is an OCC write conflict: a stale writer must retry its entire transaction,
/// so no lower cursor can commit after a higher one. This journal starts at
/// installation; existing documents are deliberately not assigned fake history.
pub(crate) async fn record(store: &NamespaceView, collection: u32, doc: &str) -> Result<u64> {
    let existing = number(store, &doc_key(collection, doc)).await?;
    if existing != 0 {
        return Ok(existing);
    }
    // DEFRALEVEL(S6): Drop head get+1+set (collection-wide RMW serializing creates); write only per-doc marker, sequencer assigns cursor
    let cursor = number(store, &head_key(collection))
        .await?
        .checked_add(1)
        .ok_or_else(|| Error::Serialization("arrival cursor exhausted".into()))?;
    store
        .set(&head_key(collection), &cursor.to_be_bytes())
        .await?;
    store
        .set(&doc_key(collection, doc), &cursor.to_be_bytes())
        .await?;
    store
        .set(&row_key(collection, cursor), doc.as_bytes())
        .await?;
    Ok(cursor)
}

// DEFRALEVEL(S6): Read sequencer watermark, not txn-written head.
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

// DEFRALEVEL(S6): Rewrite tests against the sequencer API.
#[cfg(test)]
#[path = "../../tests/read/arrival_transactions.rs"]
mod tests;
