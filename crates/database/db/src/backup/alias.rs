//! The file's document IDs as aliases of the imported documents: checked
//! free just before their batch is saved, registered once it is saved.

use std::sync::Arc;

use storage::corekv::Store;

use super::host::DbJob;
use crate::docid::map::{get_doc_ref, set_doc_id_alias};
use crate::{Error, DB};

/// Go's text for an alias, or an imported ID, that names a stored document.
pub(super) const ALIAS_TAKEN: &str = "a document with the given ID already exists";

/// The job checking that no stored document answers to any of `aliases`.
pub(super) fn check_free<S>(database: &Arc<DB<S>>, aliases: Vec<String>) -> DbJob
where
    S: Store + 'static,
{
    let database = Arc::clone(database);
    Box::new(move || Box::pin(async move { ensure_free(&database, &aliases).await }))
}

async fn ensure_free<S>(database: &Arc<DB<S>>, aliases: &[String]) -> Result<(), String>
where
    S: Store + 'static,
{
    let txn = database
        .new_txn(true)
        .await
        .map_err(|e| format!("failed to create alias lookup transaction: {e}"))?;
    let taken = {
        let systemstore = txn
            .systemstore()
            .map_err(|e| format!("failed to get systemstore: {e}"))?;
        let mut taken = false;
        for alias in aliases {
            if get_doc_ref(&systemstore, alias)
                .await
                .map_err(|e| format!("doc-ID alias lookup failed: {e}"))?
                .is_some()
            {
                taken = true;
                break;
            }
        }
        taken
    };
    txn.discard()
        .map_err(|e| format!("failed to discard alias lookup transaction: {e}"))?;
    match taken {
        true => Err(ALIAS_TAKEN.to_string()),
        false => Ok(()),
    }
}

/// One saved document and the file's IDs for it.
#[derive(Clone)]
pub(super) struct Saved {
    pub(super) doc_id: String,
    pub(super) aliases: Vec<String>,
}

/// The job registering every alias of `saved`, documents of the collection
/// whose short ID is `collection_short_id`, in one transaction. Each alias
/// is read validated before it is set, so an alias another writer took
/// since its check fails the registration with [`ALIAS_TAKEN`] and is never
/// overwritten; the documents stay saved.
pub(super) fn register<S>(
    database: &Arc<DB<S>>,
    collection_short_id: u32,
    saved: Vec<Saved>,
) -> DbJob
where
    S: Store + 'static,
{
    let database = Arc::clone(database);
    Box::new(move || {
        Box::pin(async move {
            let saved = Arc::new(saved);
            database
                .transact_db(move |txn| {
                    let saved = Arc::clone(&saved);
                    Box::pin(async move {
                        let systemstore = txn.systemstore()?;
                        for doc in saved.iter() {
                            if doc.aliases.iter().all(|alias| *alias == doc.doc_id) {
                                continue;
                            }
                            let doc_ref = get_doc_ref(&systemstore, &doc.doc_id)
                                .await?
                                .ok_or_else(|| {
                                    Error::InvalidDocument(format!(
                                        "imported document '{}' has no identity mapping",
                                        doc.doc_id
                                    ))
                                })?;
                            for alias in doc.aliases.iter().filter(|a| **a != doc.doc_id) {
                                set_doc_id_alias(
                                    &systemstore,
                                    collection_short_id,
                                    doc_ref.doc_short_id,
                                    alias,
                                )
                                .await?;
                            }
                        }
                        Ok(())
                    })
                })
                .await
                .map_err(registration_error)
        })
    })
}

/// A failed registration in Go's words when an alias was taken meanwhile.
fn registration_error(error: Error) -> String {
    let message = error.to_string();
    match message.contains("already belongs to another document") {
        true => ALIAS_TAKEN.to_string(),
        false => format!("failed to register doc-ID alias: {message}"),
    }
}
