use super::*;

use defra_core::batch_signing::BatchCid;

/// Add `cids` to the request's batch signing session, `batch_session`, once
/// the transaction that wrote them commits. The session's scope decides
/// which kinds it keeps: composite blocks only (Go Shinzo's), or all.
///
/// Batch signing covers a Merkle root over the collected CIDs, so a write
/// that rolls back, or is retried in a fresh transaction, must contribute
/// nothing until it is durable.
pub(crate) async fn collect_on_commit(
    store: &NamespaceView,
    batch_session: Option<&str>,
    cids: impl IntoIterator<Item = (Cid, BatchCid)>,
) -> Result<(), String> {
    let Some(session_key) = batch_session.map(str::to_owned) else {
        return Ok(());
    };
    let cids: Vec<(Cid, BatchCid)> = cids.into_iter().collect();
    store
        .on_commit(storage::corekv::OnCommit::Sync(Box::new(move || {
            for (cid, kind) in cids {
                defra_core::batch_signing::batch_collect_cid(&session_key, cid, kind);
            }
        })))
        .await
        .map_err(|error| format!("Failed to collect block CIDs for batch signing: {error}"))
}
