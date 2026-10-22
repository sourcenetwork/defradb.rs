//! The conflicts of a Json field: for each path, the concurrent edits its
//! winner beat, with the blocks that made them.

use std::collections::BTreeSet;

use cid::Cid;
use crdt::stored::StoredField;
use crdt::JsonField;
use defra_core::block::Block;
use defra_core::json_crdt::JsonEdit;
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
    let field = collection
        .schema()
        .field_by_name(&options.field)
        .ok_or_else(|| {
            Error::InvalidDocument(format!(
                "{} has no field `{}`",
                options.collection, options.field
            ))
        })?;
    let invalid =
        |e: crdt::FieldCrdtError| Error::InvalidDocument(format!("`{}`: {e}", field.name));
    JsonField::conflicts(field, None).map_err(invalid)?;
    let Some(doc) = crate::docid::map::get_doc_ref(&txn.systemstore()?, &options.doc_id).await?
    else {
        return Ok(Vec::new());
    };
    let state = collection
        .read_part_unvalidated(&txn.datastore()?, doc.doc_short_id, &field.name)
        .await?;
    let conflicts =
        JsonField::conflicts(field, state.as_ref().map(StoredField::part)).map_err(invalid)?;
    if conflicts.is_empty() {
        return Ok(Vec::new());
    }
    let heads = crate::block::builder::DocHeadsSnapshot::load(&txn.headstore()?, doc.doc_short_id)
        .await
        .map_err(Error::InvalidDocument)?
        .field_heads(&field.name);
    let wanted: Vec<(u64, &[u8])> = conflicts
        .iter()
        .map(|conflict| (conflict.priority, conflict.block.as_slice()))
        .collect();
    let cids = block_cids(&txn.blockstore()?, field, &heads, wanted).await?;
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

/// The CIDs of the field's blocks whose operation IDs `wanted` names, found
/// by walking down from `heads`, highest first, until no block left could be
/// one. An encrypted block's ID needs its plain data, so it is not found here.
async fn block_cids(
    blockstore: &datastore::NamespaceView,
    field: &schema::FieldDescription,
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
        let tagged = crdt::delta_crdt(&block.delta, field.crdt_type)
            .ok()
            .and_then(|writer| writer.tagged());
        if let (Some(tagged), Some(delta), None) = (tagged, block.delta.field(), &block.encryption)
        {
            let parent_ids: Vec<Vec<u8>> = parents.iter().map(Cid::to_bytes).collect();
            let id = tagged.op_id(delta.priority, &parent_ids, delta.data);
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
