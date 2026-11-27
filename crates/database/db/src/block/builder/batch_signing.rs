use super::*;

/// Add `cids` to the request's batch signing session, `batch_session`, once
/// the transaction that wrote them commits.
///
/// Batch signing covers a Merkle root over the collected CIDs, so a write
/// that rolls back, or is retried in a fresh transaction, must contribute
/// nothing until it is durable.
pub(crate) async fn collect_on_commit(
    store: &NamespaceView,
    batch_session: Option<&str>,
    cids: impl IntoIterator<Item = Cid>,
) -> Result<(), String> {
    let Some(session_key) = batch_session.map(str::to_owned) else {
        return Ok(());
    };
    let cids: Vec<Cid> = cids.into_iter().collect();
    store
        .on_commit(storage::corekv::OnCommit::Sync(Box::new(move || {
            for cid in cids {
                defra_core::batch_signing::batch_collect_cid(&session_key, cid);
            }
        })))
        .await
        .map_err(|error| format!("Failed to collect block CIDs for batch signing: {error}"))
}
