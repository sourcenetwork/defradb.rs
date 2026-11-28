//! What follows a relationship change once it stands: the document-update
//! event a grant announces, and the pass that submits what a transaction
//! staged for a remote provider.

use std::sync::Arc;

use storage::corekv::Store;

use crate::database::DB;

/// The longest a commit waits for the provider to take what its transaction
/// staged. Past it the commit returns, and what is not settled stays owed
/// for the next pass.
pub const OWED_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// Announce the document's latest state so subscribers re-evaluate access.
///
/// Three surfaces treat an unpublishable event differently. Go
/// (`internal/db/db_dac.go:129-134`, calling `publishDocUpdateEvent` in
/// `internal/db/db.go`) makes a doc that is absent from the store a silent
/// no-op and propagates only genuine storage errors. The FFI propagates
/// both (`crates/entry-points/ffi/src/acp/dac.rs`). The node deliberately
/// warns on both and continues: the relationship is already persisted by
/// the time this runs and the event is only a best-effort notification, so
/// surfacing an error here would tell the caller the grant failed when it
/// did not.
pub(super) async fn publish_document_update<S>(database: &DB<S>, collection_id: &str, doc_id: &str)
where
    S: Store + 'static,
{
    let Some(bus) = database.event_bus() else {
        return;
    };
    match crate::block::reader::read_latest_composite_block(database, doc_id).await {
        Ok(result) => bus.publish(events::Message::update(events::Update::new(
            doc_id.to_string(),
            result.cid,
            collection_id.to_string(),
            result.block,
            false,
            false,
        ))),
        Err(error) => tracing::warn!(
            doc_id,
            error,
            "failed to publish document update after DAC grant"
        ),
    }
}

/// Submit what the committed transaction `staged_by` now owes the provider:
/// the changes on the objects it staged on, within
/// [`OWED_SETTLE_DEADLINE`]. The commit stands whatever this comes to.
pub async fn settle_owed_changes(acp: Arc<dyn acp::DocumentACP>, staged_by: u64) {
    // The node's drainer takes up whatever this wait leaves.
    acp.owed_changed();
    let pass = acp.settle_owed_changes(acp::OwedScope::Transaction(staged_by));
    match workers::timeout(OWED_SETTLE_DEADLINE, pass).await {
        Ok(Ok(settlement)) if settlement.provider_unavailable => tracing::warn!(
            settled = settlement.settled,
            "ACP provider unavailable; owed relationship changes stay staged"
        ),
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(
            %error,
            "Could not settle owed ACP relationship changes; they stay owed"
        ),
        // Dropping the pass leaves its submission in flight. It cannot land
        // after a later change of its tuple, because the provider takes one
        // signer's requests in order (`DocumentACP::submit_owed_change`).
        Err(workers::Error::TimedOut) => tracing::warn!(
            deadline = ?OWED_SETTLE_DEADLINE,
            "ACP provider did not take the staged relationship changes in time; they stay owed"
        ),
        Err(error) => tracing::warn!(
            %error,
            "Could not wait for the staged relationship changes to settle; they stay owed"
        ),
    }
}
