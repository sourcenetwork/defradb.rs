//! SE artifact generation during P2P merge.
//!
//! When documents are received via replication, the receiving node generates
//! SE artifacts if the collection has encrypted indexes and the node has an
//! SE encryption key configured. This ensures replicated documents are
//! searchable on the receiving node.

use rapidhash::RapidHashMap;

use document::NormalValue;
use schema::CollectionVersion;
use storage::corekv::{Result, Writer};

use crate::merge::se::{generate_doc_artifacts, store_artifacts};

/// Generate and store SE artifacts for a replicated document.
///
/// Called after a successful composite merge when the receiving node
/// has an SE encryption key configured. Generates search tags for
/// all encrypted-indexed fields and stores them in the datastore.
pub(crate) async fn generate_merge_artifacts<S: Writer>(
    store: &mut S,
    schema: &CollectionVersion,
    doc_id: &str,
    field_values: &RapidHashMap<String, NormalValue>,
    enc_key: &[u8],
    identity_pubkey: Option<&[u8]>,
) -> Result<usize> {
    let encrypted_indexes = &schema.encrypted_indexes;
    if encrypted_indexes.is_empty() {
        return Ok(0);
    }

    let artifacts = generate_doc_artifacts(
        &schema.collection_id,
        doc_id,
        encrypted_indexes,
        &[], // all encrypted fields
        field_values,
        identity_pubkey,
        enc_key,
    )?;

    if artifacts.is_empty() {
        return Ok(0);
    }

    let count = artifacts.len();
    store_artifacts(store, &artifacts).await?;

    tracing::debug!(
        doc_id = %doc_id,
        collection_id = %schema.collection_id,
        artifact_count = count,
        "Generated SE artifacts for replicated document"
    );

    Ok(count)
}

/// Post-commit action: regenerate and push this document's SE artifacts to the
/// collection's replicators. Runs only after the merge transaction has committed,
/// because `regenerate_and_push_se_artifacts` re-reads the document through a fresh
/// read transaction and would not see an uncommitted merge.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct SeRepushAction {
    repusher: std::sync::Arc<dyn crate::merge::SeArtifactRepusher>,
    collection_id: String,
    doc_id: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl SeRepushAction {
    pub(crate) fn new(
        repusher: std::sync::Arc<dyn crate::merge::SeArtifactRepusher>,
        collection_id: String,
        doc_id: String,
    ) -> Self {
        Self {
            repusher,
            collection_id,
            doc_id,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl super::hook::CompositePostCommitAction for SeRepushAction {
    async fn run(self: Box<Self>) -> std::result::Result<(), super::MergeError> {
        self.repusher
            .regenerate_and_push_se_artifacts(&self.collection_id, &self.doc_id)
            .await;
        Ok(())
    }
}
