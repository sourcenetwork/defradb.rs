use datastore::NamespaceView;
use rapidhash::RapidHashSet;
use storage::corekv::IterOptions;

use crate::collection::Collection;

/// Exact replay must cost the selected set, not the collection's retained
/// history. Resolve canonical, live documents in the caller's read snapshot;
/// aliases and deleted documents were not members of the former blob scan.
pub(super) async fn document_ids(
    collection: &Collection,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    selected: Option<&RapidHashSet<&str>>,
) -> Result<Vec<(u64, String)>, String> {
    let mut documents = Vec::new();
    if let Some(selected) = selected {
        for doc_id in selected {
            let short_id = crate::docid::map::get_doc_short_id(
                systemstore,
                collection.resolved_root_id(),
                doc_id,
            )
            .await
            .map_err(|error| format!("doc-ID mapping lookup failed: {error}"))?;
            let Some(short_id) = short_id else { continue };
            let canonical = crate::docid::map::get_doc_id(systemstore, short_id)
                .await
                .map_err(|error| format!("doc-ID mapping lookup failed: {error}"))?;
            if canonical.as_deref() != Some(*doc_id)
                || !datastore
                    .has(&collection.doc_key(short_id))
                    .await
                    .map_err(|error| format!("failed to read selected document: {error}"))?
            {
                continue;
            }
            documents.push((short_id, (*doc_id).to_owned()));
        }
        documents.sort_unstable_by_key(|(short_id, _)| *short_id);
        return Ok(documents);
    }

    let prefix = format!("/d/{}/", collection.collection_id()).into_bytes();
    let prefix_len = prefix.len();
    let mut iterator = datastore
        .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
        .await
        .map_err(|error| format!("failed to iterate datastore: {error}"))?;
    let mut short_ids = Vec::new();
    while let Some(pair) = iterator
        .next()
        .await
        .map_err(|error| format!("datastore iteration error: {error}"))?
    {
        if let Ok(short_id) =
            storage::keys::doc_id_index::decode_doc_short_id(&pair.key[prefix_len..])
        {
            short_ids.push(short_id);
        }
    }
    iterator
        .close()
        .await
        .map_err(|error| format!("datastore close error: {error}"))?;
    for short_id in short_ids {
        if let Some(doc_id) = crate::docid::map::get_doc_id(systemstore, short_id)
            .await
            .map_err(|error| format!("doc-ID mapping lookup failed: {error}"))?
        {
            documents.push((short_id, doc_id));
        }
    }
    Ok(documents)
}
