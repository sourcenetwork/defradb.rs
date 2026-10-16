//! Whether a replicator peer has acknowledged a document's current heads.

use document::DocID;
use storage::corekv::Store;

use crate::merge::push_docs_common::{load_collection_head_cids, load_latest_composite_head_cids};
use crate::DB;

/// For each of `doc_ids` in `collection_name`, whether `peer_id` acknowledged
/// the heads this node currently holds for it, with no retry outstanding.
///
/// A `@branchable` collection replicates at collection scope, so its
/// documents are acknowledged together with the collection heads. A newer
/// local write makes a document unacknowledged until the peer acknowledges
/// again; a missing document is never acknowledged.
pub async fn replication_acknowledged<S: Store + 'static>(
    db: &DB<S>,
    peer_id: &str,
    collection_name: &str,
    doc_ids: &[String],
) -> Result<Vec<bool>, String> {
    let collection = db
        .get_collection(collection_name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("collection {collection_name} not found"))?;
    let collection_id = collection.collection_id().to_string();
    let peerstore = storage::stores::Peerstore::new(db.store().clone());
    let txn = db.new_txn(true).await.map_err(|error| error.to_string())?;
    let systemstore = txn.systemstore().map_err(|error| error.to_string())?;
    let headstore = txn.headstore().map_err(|error| error.to_string())?;
    let blockstore = txn.blockstore().map_err(|error| error.to_string())?;

    let collection_acknowledged = if collection.schema().is_branchable {
        let short_id =
            crate::collection::require_persisted_collection_short_id(&systemstore, &collection_id)
                .await
                .map_err(|error| error.to_string())?;
        let current = sorted(load_collection_head_cids(&headstore, short_id).await?);
        let acked = peerstore
            .acknowledged_heads(peer_id, &collection_id, true)
            .await
            .map_err(|error| error.to_string())?;
        Some(acked == Some(current))
    } else {
        None
    };

    let mut acknowledged = Vec::with_capacity(doc_ids.len());
    for doc_id in doc_ids {
        let Ok(parsed) = doc_id.parse::<DocID>() else {
            acknowledged.push(false);
            continue;
        };
        let Some((doc_short_id, canonical)) = collection
            .resolve_doc_identity(&systemstore, &parsed)
            .await
            .map_err(|error| error.to_string())?
        else {
            acknowledged.push(false);
            continue;
        };
        if let Some(collection_acknowledged) = collection_acknowledged {
            acknowledged.push(collection_acknowledged);
            continue;
        }
        let current =
            sorted(load_latest_composite_head_cids(&headstore, &blockstore, doc_short_id).await);
        let acked = peerstore
            .acknowledged_heads(peer_id, &canonical.to_string(), false)
            .await
            .map_err(|error| error.to_string())?;
        acknowledged.push(acked == Some(current));
    }
    Ok(acknowledged)
}

fn sorted(cids: Vec<cid::Cid>) -> Vec<String> {
    let mut heads: Vec<String> = cids.iter().map(ToString::to_string).collect();
    heads.sort_unstable();
    heads
}
