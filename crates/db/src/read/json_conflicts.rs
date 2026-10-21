//! The conflicts of a Json field: for each path, the concurrent edits its
//! winner beat, with the blocks that made them.

use std::collections::BTreeSet;

use cid::Cid;
use defra_core::block::Block;
use defra_core::json_crdt::{self, JsonEdit};
use defra_core::CrdtDelta;
use query::fetcher::{JsonConflict, JsonConflictOptions};
use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use storage::corekv::Store;

use crate::error::{Error, Result};
use crate::txn::DbTxn;

pub(crate) async fn read<S: Store>(
    txn: &DbTxn<S>,
    options: &JsonConflictOptions,
) -> Result<Vec<JsonConflict>> {
    let collection = txn
        .get_collection(&options.collection)
        .await?
        .ok_or_else(|| Error::CollectionNotFound(options.collection.clone()))?;
    if !collection
        .schema()
        .fields
        .iter()
        .any(|field| field.name == options.field && field.crdt_type.is_json())
    {
        return Err(Error::InvalidDocument(format!(
            "`{}` is not a `@crdt(type: json)` field of {}",
            options.field, options.collection
        )));
    }
    let Some(doc) = crate::docid::map::get_doc_ref(&txn.systemstore()?, &options.doc_id).await?
    else {
        return Ok(Vec::new());
    };
    let frontier = collection
        .read_json_frontier(&txn.datastore()?, doc.doc_short_id, &options.field)
        .await?;
    let conflicts = frontier.conflicts();
    if conflicts.is_empty() {
        return Ok(Vec::new());
    }
    let heads = crate::block::builder::DocHeadsSnapshot::load(&txn.headstore()?, doc.doc_short_id)
        .await
        .map_err(Error::InvalidDocument)?
        .field_heads(&options.field);
    let wanted: Vec<(u64, &[u8])> = conflicts
        .iter()
        .map(|conflict| (conflict.priority, conflict.block.as_slice()))
        .collect();
    let cids = block_cids(&txn.blockstore()?, &heads, wanted).await?;
    Ok(conflicts
        .into_iter()
        .map(|conflict| JsonConflict {
            cid: cids.get(&conflict.block).map(Cid::to_string),
            path: conflict.path,
            height: conflict.priority,
            value: match &conflict.edit {
                JsonEdit::Set(value) => Some(value.clone()),
                JsonEdit::Map => Some(serde_json::Value::Object(Default::default())),
                JsonEdit::Delete => None,
            },
            deleted: conflict.edit == JsonEdit::Delete,
        })
        .collect())
}

/// The CIDs of the field's blocks whose IDs `wanted` names, found by walking
/// down from `heads`, highest first, until no block left could be one. An
/// encrypted block's ID needs its plain ops, so it is not found here.
async fn block_cids(
    blockstore: &datastore::NamespaceView,
    heads: &[Cid],
    mut wanted: Vec<(u64, &[u8])>,
) -> Result<RapidHashMap<Vec<u8>, Cid>> {
    wanted.sort_unstable();
    let mut found = RapidHashMap::default();
    let mut seen = RapidHashSet::new();
    let mut waiting: BTreeSet<(u64, Cid)> = BTreeSet::new();
    let load = |cid: Cid| async move {
        let bytes = blockstore
            .get(&cid.to_bytes())
            .await
            .map_err(Error::Storage)?
            .ok_or_else(|| Error::InvalidDocument(format!("block {cid} not found")))?;
        Block::from_dag_cbor(&bytes).map_err(|e| Error::Serialization(e.to_string()))
    };
    for head in heads {
        if seen.insert(*head) {
            waiting.insert((load(*head).await?.delta.priority(), *head));
        }
    }
    while let Some((priority, next)) = waiting.pop_last() {
        if wanted.first().is_none_or(|(lowest, _)| *lowest > priority) {
            break;
        }
        let block = load(next).await?;
        let parents = block.heads.clone().unwrap_or_default();
        if let (CrdtDelta::Json(payload), None) = (&block.delta, &block.encryption) {
            let parent_bytes: Vec<Vec<u8>> = parents.iter().map(Cid::to_bytes).collect();
            let id = json_crdt::block_id(payload.priority, &parent_bytes, &payload.data);
            wanted.retain(|(_, block)| *block != id.as_slice());
            found.insert(id, next);
        }
        for parent in parents {
            if seen.insert(parent) {
                waiting.insert((load(parent).await?.delta.priority(), parent));
            }
        }
    }
    Ok(found)
}
