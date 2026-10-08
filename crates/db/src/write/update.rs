//! Document update: the one path every mutator's update goes through.

use bytes::Bytes;
use cid::Cid;
use document::Document;
use query::error::{QueryError, Result};
use rapidhash::RapidHashSet;
use storage::corekv::Store;

use crate::block::builder::{
    compute_document_blocks, insert_computed_blocks, plan_document_blocks, DocStorageIdentity,
};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::write::create::TxnStores;
use crate::write::persist::{
    register_block_doc_id_mappings, write_branchable_collection_block, write_local_update,
    write_local_update_deferred,
};
use defra_core::encryption::get_encryption_config;
use defra_core::signing::get_signing_config;

/// When an update's counter deltas reach the CRDT accumulation store.
pub(crate) enum CounterWrite {
    /// In this transaction, under the caller's per-document guard.
    Now,
    /// At the explicit transaction's commit-time finalize, which the caller
    /// has recorded the deltas for (#1044).
    AtCommit,
}

/// The blocks an update produced, for the mutator to publish after commit.
pub(crate) struct UpdatedDoc {
    pub cid: Cid,
    pub block: Bytes,
    pub collection_block: Option<(Cid, Bytes)>,
}

/// Generate embeddings for `doc` whose source fields changed, and add the
/// generated fields to `modified_fields`.
pub(crate) async fn embed_update<S: Store>(
    db: &DB<S>,
    collection: &Collection,
    doc: &mut Document,
    modified_fields: &mut RapidHashSet<String>,
) -> Result<()> {
    let generated = crate::search::set_embedding(
        &collection.schema().vector_embeddings,
        doc,
        false,
        Some(modified_fields),
        &db.options().embedding_config(),
    )
    .await
    .map_err(|e| QueryError::execution(format!("embedding error: {}", e)))?;
    modified_fields.extend(generated);
    Ok(())
}

/// Update `doc`'s `modified_fields` through the transaction's `stores`.
///
/// Every new block is computed before any is stored, on top of the heads read
/// in this transaction. `doc` must carry its canonical DocID; its counter
/// fields hold the authoritative values afterwards when `counters` is
/// [`CounterWrite::Now`].
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_document<S: Store + 'static>(
    db: &DB<S>,
    stores: &TxnStores,
    collection_name: &str,
    collection: &Collection,
    index_manager: &IndexManager,
    doc: &mut Document,
    doc_short_id: u64,
    modified_fields: &RapidHashSet<String>,
    counters: CounterWrite,
) -> Result<UpdatedDoc> {
    let store_error = |e: crate::error::Error| QueryError::execution(e.to_string());
    let block_error = |e: String| {
        QueryError::execution(format!(
            "failed to write document blocks for update on collection {}: {}",
            collection_name, e
        ))
    };
    let TxnStores {
        datastore,
        systemstore,
        blockstore,
        headstore,
    } = stores;

    db.validate_downsample_write(
        datastore,
        systemstore,
        collection.schema(),
        doc,
        Some(modified_fields),
    )
    .await
    .map_err(store_error)?;

    // Explicit config from the mutation only. A document created encrypted
    // keeps its encryption because the plan inherits it from the previous
    // block, matching Go's determineBlockEncryption.
    let enc_config = get_encryption_config();
    let sign_config = get_signing_config();
    let identity = DocStorageIdentity::new(collection.resolved_root_id(), doc_short_id);
    let plan = plan_document_blocks(
        blockstore,
        headstore,
        doc,
        identity,
        Some(modified_fields),
        enc_config.as_ref(),
        db.kms().as_ref(),
    )
    .await
    .map_err(block_error)?;
    let blocks = compute_document_blocks(
        doc,
        collection.version_id(),
        identity,
        &plan,
        sign_config.as_ref(),
    )
    .map_err(block_error)?;

    match counters {
        CounterWrite::Now => {
            write_local_update(datastore, collection, doc, doc_short_id, index_manager).await?
        }
        CounterWrite::AtCommit => {
            write_local_update_deferred(datastore, collection, doc, doc_short_id, index_manager)
                .await?
        }
    }
    insert_computed_blocks(blockstore, headstore, &blocks)
        .await
        .map_err(block_error)?;
    let result = blocks.block_result;
    register_block_doc_id_mappings(systemstore, &result, &result.doc_id).await?;

    let collection_block = write_branchable_collection_block(
        db,
        collection_name,
        collection,
        blockstore,
        headstore,
        result.cid,
        sign_config.as_ref(),
    )
    .await?;

    Ok(UpdatedDoc {
        cid: result.cid,
        block: result.block,
        collection_block,
    })
}
