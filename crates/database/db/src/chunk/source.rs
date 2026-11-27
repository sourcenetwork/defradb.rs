//! The parent commit a chunk names in `sourceCID`, and whether it merged here.

use cid::Cid;
use datastore::NamespaceView;
use defra_core::block::Block;
use defra_core::cid_encoding::cid_bytes;
use document::{Document, NormalValue};
use schema::CHUNK_SOURCE_CID_FIELD;
use storage::merge::head_set::Set;

use crate::error::{Error, Result};

/// The live heads of `field` in a document's head set, with their priorities.
pub(crate) fn field_heads(set: &Set, field: &str) -> Vec<(Cid, u64)> {
    set.live()
        .filter_map(|(name, priority)| {
            let (head_field, cid) = crate::block::heads::document::parse_name(name)?;
            (head_field == field).then_some((cid, priority))
        })
        .collect()
}

/// The head whose value shows: the highest priority, then the one holding
/// `value`, then the largest CID, so every node picks the same one.
pub(crate) async fn winning_commit(
    blockstore: &NamespaceView,
    heads: &[(Cid, u64)],
    value: Option<&NormalValue>,
) -> Result<Option<Cid>> {
    let Some(top) = heads.iter().map(|(_, priority)| *priority).max() else {
        return Ok(None);
    };
    let tied: Vec<Cid> = heads
        .iter()
        .filter(|(_, priority)| *priority == top)
        .map(|(cid, _)| *cid)
        .collect();
    let mut holding = Vec::new();
    if tied.len() > 1 {
        for cid in &tied {
            if stored_value(blockstore, cid).await?.as_ref() == value {
                holding.push(*cid);
            }
        }
    }
    let candidates = if holding.is_empty() { tied } else { holding };
    Ok(candidates.into_iter().max_by_key(Cid::to_string))
}

async fn stored_value(blockstore: &NamespaceView, cid: &Cid) -> Result<Option<NormalValue>> {
    let Some(bytes) = blockstore
        .get(&cid_bytes(cid))
        .await
        .map_err(Error::Storage)?
    else {
        return Ok(None);
    };
    let block = Block::from_dag_cbor(&bytes)
        .map_err(|error| Error::Other(format!("a head block does not decode: {error}")))?;
    Ok(block
        .delta
        .field()
        .and_then(|delta| ciborium::from_reader(delta.data).ok()))
}

/// Whether the commit `source` of `parent` has merged here. `merging` is
/// the commit a merge is writing now, whose ownership commits with it.
pub(crate) async fn is_merged(
    systemstore: &NamespaceView,
    parent: &str,
    source: &str,
    merging: Option<&Cid>,
) -> Result<bool> {
    if merging.is_some_and(|cid| cid.to_string() == source) {
        return Ok(true);
    }
    crate::docid::map::owns_block(systemstore, source, parent).await
}

/// Records `source` as the commit `doc` was cut from.
pub(crate) fn set_source(doc: &mut Document, source: &Cid) -> Result<()> {
    doc.set(
        CHUNK_SOURCE_CID_FIELD,
        NormalValue::String(source.to_string()),
    )
    .map_err(|error| Error::Other(format!("chunk document: {error}")))
}

/// The memo entry through which a backup import hands a chunk its `sourceCID`.
pub(crate) fn imported_source_request(
    chunks_col: u32,
    parent: &str,
    hash: &str,
) -> query::prepared::PreparedInputRequest {
    query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Allocation,
        [
            b"chunk-source-v1".as_slice(),
            &chunks_col.to_be_bytes(),
            parent.as_bytes(),
            b"/",
            hash.as_bytes(),
        ]
        .concat(),
    )
}
