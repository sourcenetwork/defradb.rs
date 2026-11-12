use crate::error::{Error, Result};
use datastore::NamespaceView;
use query::fetcher::{DocumentUpdate, DocumentUpdateOptions, DocumentUpdatePage};
use serde::{Deserialize, Serialize};
use storage::corekv::AppendLog;
use storage::corekv::Store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Update {
    pub doc_id: String,
    pub cid: String,
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
            &AppendLog::new(
                format!("/arrival/updates/{collection}/head").into_bytes(),
                format!("/arrival/updates/{collection}/row/").into_bytes(),
            ),
            &row,
            Some(&once_key(collection, doc, cid)),
        )
        .await
        .map_err(Error::Storage)
}

pub(crate) async fn read<S>(
    txn: &crate::txn::DbTxn<S>,
    options: &DocumentUpdateOptions,
) -> Result<DocumentUpdatePage>
where
    S: Store,
{
    let collection = txn
        .get_collection(&options.collection)
        .await?
        .ok_or_else(|| Error::CollectionNotFound(options.collection.clone()))?
        .resolved_root_id();
    let store = txn.systemstore()?;
    let head = match store
        .get(format!("/arrival/updates/{collection}/head").as_bytes())
        .await?
    {
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
    let mut entries = Vec::new();
    for cursor in (options.after..next).map(|cursor| cursor + 1) {
        let key = format!("/arrival/updates/{collection}/row/{cursor:020}");
        let bytes = store
            .get(key.as_bytes())
            .await?
            .ok_or_else(|| Error::Serialization("missing update row".into()))?;
        let row: Update = serde_json::from_slice(&bytes)
            .map_err(|error| Error::Serialization(error.to_string()))?;
        entries.push(DocumentUpdate {
            cursor,
            doc_id: row.doc_id,
            cid: row.cid,
        });
    }
    Ok(DocumentUpdatePage {
        head,
        next,
        entries,
    })
}

#[cfg(test)]
#[path = "../../tests/read/update_transactions.rs"]
mod tests;
