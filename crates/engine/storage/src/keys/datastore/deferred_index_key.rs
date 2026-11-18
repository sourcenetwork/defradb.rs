use crate::corekv::AppendLog;

/// The commit-ordered log of documents a deferred index owes an update:
/// `/iplog/<collShortID>/<indexID>/head` and `.../row/<position>`.
///
/// Outside the index's entry prefix, because regolith classifies a log by its
/// key's family and only `append` and `truncate_log` may write its keys.
pub fn index_pending_log(collection_short_id: u32, index_id: u32) -> AppendLog {
    let base = format!("/iplog/{collection_short_id}/{index_id}/");
    AppendLog::new(
        format!("{base}head").into_bytes(),
        format!("{base}row/").into_bytes(),
    )
}

/// The prefix of every position record of one deferred index.
pub fn index_position_prefix(collection_short_id: u32, index_id: u32) -> Vec<u8> {
    format!("/iprec/{collection_short_id}/{index_id}/").into_bytes()
}

/// Where a deferred index's entries for one document are:
/// `/iprec/<collShortID>/<indexID>/<docShortID as 8 big-endian bytes>`.
///
/// Outside the entry prefix, so an index scan never sees it.
pub fn index_position_key(collection_short_id: u32, index_id: u32, doc_short_id: u64) -> Vec<u8> {
    let mut key = index_position_prefix(collection_short_id, index_id);
    key.extend_from_slice(&doc_short_id.to_be_bytes());
    key
}
