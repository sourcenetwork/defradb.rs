//! Moving a deferred index's entries to the values its log owes.

use std::sync::Arc;

use storage::corekv::{IterOptions, RetryAttemptError, RetryAttemptValue, Store};
use storage::keys::{index_pending_log, index_position_key};

use crate::collection::loader::load_collection_from_systemstore;
use crate::database::DB;
use crate::index::error::{Error, Result};
use crate::index::IndexManager;

/// The most log entries one drain transaction takes, so a backlog commits in
/// bounded transactions.
const MAX_PLAN: usize = 256;

/// What one drain will apply, read from a snapshot.
struct Plan {
    first: u64,
    through: u64,
    docs: Vec<u64>,
}

/// Drains up to [`MAX_PLAN`] entries of the index `index_id` of the
/// collection `name`. True when it took some, so the caller drains again.
pub(crate) async fn drain_index<S>(db: &DB<S>, name: &str, index_id: u32) -> Result<bool>
where
    S: Store + 'static,
{
    let Some(collection) = db
        .get_collection(name)
        .map_err(|e| Error::Other(e.to_string()))?
    else {
        return Ok(false);
    };
    let short_id = collection.resolved_root_id();
    let Some(plan) = plan(db, short_id, index_id).await? else {
        return Ok(false);
    };
    let (name, plan) = (name.to_string(), Arc::new(plan));
    datastore::transact(
        db.store().as_ref(),
        db.options().retry_options(),
        move |txn| {
            let (name, plan) = (name.clone(), Arc::clone(&plan));
            Box::pin(async move {
                apply(txn, &name, short_id, index_id, &plan)
                    .await
                    .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        },
    )
    .await
    .map_err(|error| Error::Other(format!("deferred index drain: {error:?}")))?;
    Ok(true)
}

/// The oldest entries of the log, read on a snapshot. Entries committed after
/// it take later positions, so they stay owed.
async fn plan<S>(db: &DB<S>, short_id: u32, index_id: u32) -> Result<Option<Plan>>
where
    S: Store + 'static,
{
    let snapshot = db
        .new_txn(true)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let read = async {
        let datastore = snapshot
            .datastore()
            .map_err(|e| Error::Other(e.to_string()))?;
        let log = index_pending_log(short_id, index_id);
        let mut iter = datastore
            .iterator(IterOptions::default().with_prefix(log.entry_prefix().to_vec()))
            .await
            .map_err(Error::Storage)?;
        let mut entries = Vec::new();
        while entries.len() < MAX_PLAN {
            let Some(pair) = iter.next().await.map_err(Error::Storage)? else {
                break;
            };
            entries.push((position(&pair.key)?, doc_short_id(&pair.value)?));
        }
        iter.close().await.map_err(Error::Storage)?;
        Ok::<_, Error>(entries)
    };
    let entries = read.await;
    let _ = snapshot.discard();
    let entries = entries?;
    let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
        return Ok(None);
    };
    let (first, through) = (first.0, last.0);
    let mut docs: Vec<u64> = entries.into_iter().map(|(_, doc)| doc).collect();
    docs.sort_unstable();
    docs.dedup();
    Ok(Some(Plan {
        first,
        through,
        docs,
    }))
}

/// Moves each planned document's entries to its live value and truncates the
/// log through the plan. The position record is the one read that decides: a
/// drain racing this one on a document makes one of them run again.
async fn apply(
    txn: &mut datastore::BasicTxn,
    name: &str,
    short_id: u32,
    index_id: u32,
    plan: &Plan,
) -> Result<()> {
    let datastore = txn.datastore();
    let log = index_pending_log(short_id, index_id);
    // Read inside the transaction, so a drop of the index commits first or
    // makes this run again and find it gone.
    let collection = load_collection_from_systemstore(&txn.systemstore(), name)
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let desc = collection.as_ref().and_then(|collection| {
        collection
            .write_indexes()
            .iter()
            .find(|index| index.id == index_id)
            .cloned()
    });
    let (Some(collection), Some(desc)) = (collection, desc) else {
        return datastore
            .truncate_log(&log, plan.first..=plan.through)
            .await
            .map_err(Error::Storage);
    };
    let schema = collection.schema();
    let manager = IndexManager::from_indexes(
        short_id,
        schema,
        std::slice::from_ref(&desc),
        collection.crdts(),
    )?;
    let index = manager
        .get_index(&desc.name)
        .ok_or_else(|| Error::Other(format!("index '{}' not found", desc.name)))?;
    for &doc in &plan.docs {
        let record = index_position_key(short_id, index_id, doc);
        let old = IndexManager::recorded(
            datastore
                .get(&record)
                .await
                .map_err(Error::Storage)?
                .as_deref(),
        )?;
        let new = match collection
            .read_hint(&datastore, doc)
            .await
            .map_err(|e| Error::Other(e.to_string()))?
        {
            Some(live) => IndexManager::entry_keys(
                index,
                doc,
                &manager.extract_index_values(&live, &desc, schema)?,
            )?,
            None => Vec::new(),
        };
        if old == new {
            continue;
        }
        for key in old.iter().filter(|key| !new.contains(key)) {
            datastore.delete(key).await.map_err(Error::Storage)?;
        }
        for key in new.iter().filter(|key| !old.contains(key)) {
            datastore.set(key, &[]).await.map_err(Error::Storage)?;
        }
        if new.is_empty() {
            datastore.delete(&record).await.map_err(Error::Storage)?;
        } else {
            datastore
                .set(&record, &super::encode_record(&new)?)
                .await
                .map_err(Error::Storage)?;
        }
    }
    datastore
        .truncate_log(&log, plan.first..=plan.through)
        .await
        .map_err(Error::Storage)
}

/// A log entry key ends in its position as 20 decimal digits.
fn position(key: &[u8]) -> Result<u64> {
    key.len()
        .checked_sub(20)
        .and_then(|start| std::str::from_utf8(&key[start..]).ok())
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| Error::Other("deferred index log entry without a position".into()))
}

fn doc_short_id(value: &[u8]) -> Result<u64> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| Error::Other("deferred index log entry is not a document short id".into()))
}
