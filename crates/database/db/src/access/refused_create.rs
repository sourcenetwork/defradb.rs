//! Release the reservations of local gated creates Vera refused.

use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, ExternalDocumentAcp};
use storage::corekv::Store;

use crate::{Error, Result, DB};

const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(60);

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Recover refused creates on startup and handle new refusals on the database pool.
    pub fn start_refused_create_cleanup(
        self: &Arc<Self>,
        gate: Arc<ExternalDocumentAcp<S>>,
    ) -> workers::Background {
        let database = Arc::clone(self);
        workers::spawn_background(self.pool(), "db::purge_refused_creates", move || {
            purge_refused_creates(gate, database)
        })
    }

    /// Release the row and its reservations after a final local create refusal.
    /// The refusal is consumed in the same transaction as the purge.
    pub async fn purge_refused_create(
        self: &Arc<Self>,
        refused: acp::PendingRegistration,
    ) -> Result<()> {
        if !refused.needs_create_cleanup() {
            return Ok(());
        }
        let db = Arc::clone(self);
        self.transact_db(move |txn| {
            let db = Arc::clone(&db);
            let refused = refused.clone();
            Box::pin(async move {
                let acpstore = txn.acpstore()?;
                let registrations = acp::PendingRegistrations::new(&acpstore);
                let (policy, resource, object) =
                    (&refused.policy_id, &refused.resource_name, &refused.doc_id);
                if registrations.get(policy, resource, object).await?.as_ref() != Some(&refused) {
                    return Ok(());
                }
                let systemstore = txn.systemstore()?;
                let doc_ref = crate::docid::map::get_doc_ref(&systemstore, object).await?;
                if let Some(doc_ref) = doc_ref
                    .filter(|doc_ref| Some(doc_ref.doc_short_id) == refused.local_create_short_id)
                {
                    let known = db
                        .find_collection_by_root_id(doc_ref.collection_short_id)
                        .ok_or_else(|| {
                            Error::Other(format!(
                                "cannot load collection {} to clean up refused create {object}",
                                doc_ref.collection_short_id
                            ))
                        })?;
                    let collection = txn
                        .load_collection_for_write_attempt(known.name(), &known)
                        .await?;
                    let doc_id = document::DocID::from_string(object)?;
                    if db.purge_documents(txn, &collection, &[doc_id]).await? != 1 {
                        return Err(Error::Other(format!(
                            "refused create {object} does not resolve in collection {}",
                            collection.name()
                        )));
                    }
                    acp::OwedChanges::new(
                        &acpstore,
                        db.store().clone(),
                        acp::StagedCount::default(),
                    )
                    .drop_unsent_on_refused_create(policy, resource, object)
                    .await?;
                }
                registrations.clear(policy, resource, object).await?;
                Ok(())
            })
        })
        .await
    }
}

/// Scan at startup and after final refusals. A failed purge leaves its
/// registration in place for the next pass.
async fn purge_refused_creates<S>(gate: Arc<ExternalDocumentAcp<S>>, database: Arc<DB<S>>)
where
    S: Store + 'static,
{
    let mut delay = FIRST_RETRY;
    loop {
        if purge_refused_pass(&gate, &database).await {
            delay = FIRST_RETRY;
            gate.refused_create_committed().await;
        } else {
            let _ = workers::timeout(delay, gate.refused_create_committed()).await;
            delay = (delay * 2).min(MAX_RETRY);
        }
    }
}

async fn purge_refused_pass<S>(gate: &ExternalDocumentAcp<S>, database: &Arc<DB<S>>) -> bool
where
    S: Store + 'static,
{
    let mut start = None;
    let mut complete = true;
    loop {
        let (refused, next) = match gate.refused_create_page(start.as_deref()).await {
            Ok(page) => page,
            Err(error) => {
                tracing::warn!(%error, "Could not read refused ACP creates");
                return false;
            }
        };
        for registration in refused {
            let object = registration.doc_id.clone();
            match database.purge_refused_create(registration).await {
                Ok(()) => gate.owed_changed(),
                Err(error) => {
                    tracing::warn!(%error, %object, "Could not purge refused ACP create");
                    complete = false;
                }
            }
        }
        let Some(next) = next else {
            return complete;
        };
        start = Some(next);
        workers::yield_now().await;
    }
}
