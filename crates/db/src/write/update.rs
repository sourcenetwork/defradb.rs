//! The one update pipeline. Every update, whoever owns its transaction,
//! writes its document's changed parts with [`write_local`], then builds
//! every block before storing any with [`write_blocks`]. The mutators only
//! own the transaction, the document's identity and heads, events and commit.

use bytes::Bytes;
use cid::Cid;
use defra_core::RequestContext;
use document::Document;
use query::prepared::PreparedInputMemo;
use query::update_parts::Run;
use storage::corekv::Store;

use crate::block::builder::{
    build_document_blocks, insert_computed_blocks, DocHeadsSnapshot, DocStorageIdentity,
};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::write::autocommit::helpers::{
    register_block_doc_id_mappings, write_branchable_collection_block, write_local_update,
};
use crate::write::WriteStores;

/// The document an update writes, by its storage short ID and its DocID.
pub(crate) struct Target<'a> {
    pub collection_name: &'a str,
    pub collection: &'a Collection,
    pub doc_short_id: u64,
    pub doc_id: &'a str,
}

/// A stored update: its composite block and, for a branchable collection,
/// the collection block.
pub(crate) struct Updated {
    pub cid: Cid,
    pub block: Bytes,
    pub collection_block: Option<(Cid, Bytes)>,
}

/// Validate the update and write the document's changed parts and indexes.
/// `modified_fields` grows by the embeddings the change regenerates.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_local<S>(
    db: &DB<S>,
    stores: &WriteStores<'_>,
    target: &Target<'_>,
    doc: &mut Document,
    index_manager: &IndexManager,
    modified_fields: &mut rapidhash::RapidHashSet<String>,
    heads: &DocHeadsSnapshot,
    run: Run,
    inputs: &PreparedInputMemo,
) -> query::error::Result<()>
where
    S: Store + 'static,
{
    db.validate_downsample_write(
        stores.datastore,
        stores.systemstore,
        target.collection,
        doc,
        Some(modified_fields),
    )
    .await
    .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    write_local_update(
        stores.datastore,
        target.collection,
        doc,
        target.doc_short_id,
        index_manager,
        modified_fields,
        heads,
        run,
        &db.options().embedding_config(),
        inputs,
    )
    .await
}

/// Build the blocks for the modified fields on top of `heads`, then store
/// them, map them to the document, and append the collection block.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_blocks<S>(
    db: &DB<S>,
    stores: &WriteStores<'_>,
    target: &Target<'_>,
    doc: &Document,
    modified_fields: &rapidhash::RapidHashSet<String>,
    heads: DocHeadsSnapshot,
    context: &RequestContext,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<Updated>
where
    S: Store + 'static,
{
    let collection = target.collection;
    let built = build_document_blocks(
        stores.blockstore,
        stores.headstore,
        doc,
        collection.schema(),
        DocStorageIdentity::new(collection.resolved_root_id(), target.doc_short_id),
        Some(modified_fields),
        context.encryption(),
        context.signing(),
        db.kms().as_ref(),
        inputs,
        Some(heads),
    )
    .await
    .map_err(|error| {
        error.into_query_error_in(&format!(
            "failed to write document blocks for update on collection {}",
            target.collection_name
        ))
    })?;
    insert_computed_blocks(
        stores.blockstore,
        stores.headstore,
        &built,
        context.batch_session(),
    )
    .await
    .map_err(|error| {
        query::error::QueryError::execution(format!(
            "failed to write document blocks for update on collection {}: {error}",
            target.collection_name
        ))
    })?;
    register_block_doc_id_mappings(stores.systemstore, &built.block_result, target.doc_id).await?;
    let collection_block = write_branchable_collection_block(
        db,
        target.collection_name,
        collection,
        stores.blockstore,
        stores.headstore,
        built.block_result.cid,
        target.doc_short_id,
        context,
        inputs,
    )
    .await?;
    Ok(Updated {
        cid: built.block_result.cid,
        block: built.block_result.block,
        collection_block,
    })
}
