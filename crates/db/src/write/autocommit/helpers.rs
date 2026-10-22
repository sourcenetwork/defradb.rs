use super::*;
use bytes::Bytes;
use defra_core::cid_encoding::CidText;

use crate::block::builder::BlockResult;
use crate::collection::Collection;
use datastore::NamespaceView;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_branchable_collection_block<S: storage::corekv::Store + 'static>(
    _db: &crate::database::DB<S>,
    collection_name: &str,
    collection: &Collection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_cid: Cid,
    _doc_short_id: u64,
    context: &RequestContext,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> query::error::Result<Option<(Cid, Bytes)>> {
    if !collection.schema().is_branchable {
        return Ok(None);
    }

    let written = write_collection_block_with_inputs(
        blockstore,
        headstore,
        collection.resolved_root_id(),
        collection.version_id(),
        doc_cid,
        context.signing(),
        context.batch_session(),
        prepared_inputs,
    )
    .await
    .map(Some)
    .map_err(|error| error.into_query_error_in(&format!("failed to write collection block for branchable mutation on collection {collection_name}")))?;

    Ok(written)
}

/// Persist a local UPDATE: write the document's changed parts and maintain
/// indexes. A counter increment is a blind operand on the counter's part of
/// the document value, its only stored copy. Embeddings over changed fields
/// use the final field values, and come from `prepared_inputs`: an embedding
/// is a remote input, so a miss ends the attempt with
/// [`query::error::QueryError::NeedsInput`] before anything is written, and
/// the caller fetches it outside the transaction and runs the update again.
/// `heads` is the snapshot the update also builds its blocks from, so the
/// document is scanned for its heads once. The caller has read the
/// collection's definition in this transaction (an attempt's
/// `load_collection_for_write_attempt`, or `validate_collection_for_write`),
/// which is what a definition change conflicts with.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_local_update(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &mut Document,
    doc_short_id: u64,
    index_manager: &IndexManager,
    modified_fields: &mut rapidhash::RapidHashSet<String>,
    heads: &crate::block::builder::DocHeadsSnapshot,
    run: query::update_parts::Run,
    embedding_config: &crate::search::EmbeddingClientConfig,
    prepared_inputs: &query::prepared::PreparedInputMemo,
) -> query::error::Result<()> {
    set_incremented_counters(datastore, collection, doc, doc_short_id).await?;
    let generated = crate::search::embedding::set_embedding_from_inputs(
        &collection.schema().vector_embeddings,
        doc,
        false,
        modified_fields,
        embedding_config,
        prepared_inputs,
    )?;
    modified_fields.extend(generated);
    collection
        .update_with_indexes_parts(
            datastore,
            doc,
            doc_short_id,
            index_manager,
            Some((&*modified_fields, heads, run)),
        )
        .await
        .map_err(|e| match e {
            crate::error::Error::DocumentNotFound(id) => {
                query::error::QueryError::document_not_found(id)
            }
            other => crate::error::index_write_query_error("update", other),
        })
}

/// Persist a local CREATE: write the document + maintain indexes. A counter's
/// initial value is its part of the document value. The caller has read the
/// collection's definition in this transaction, as for [`write_local_update`].
pub(crate) async fn write_local_create(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &Document,
    doc_short_id: u64,
    index_manager: &IndexManager,
) -> query::error::Result<()> {
    collection
        .create_with_indexes(datastore, doc, doc_short_id, index_manager)
        .await
        .map(|_| ())
        .map_err(|e| crate::error::index_write_query_error("create", e))
}

/// The heads of the document an update writes, read once for both its value
/// operands and its blocks.
pub(crate) async fn load_update_heads(
    datastore: &NamespaceView,
    doc_short_id: u64,
) -> query::error::Result<crate::block::builder::DocHeadsSnapshot> {
    crate::block::builder::DocHeadsSnapshot::load(
        &datastore.sibling(storage::namespace::Namespace::Headstore),
        doc_short_id,
    )
    .await
    .map_err(|error| {
        query::error::QueryError::execution(format!("failed to read document heads: {error}"))
    })
}

/// Register the identity of a freshly created document (Go `save()` isAdd):
/// duplicate-check the derived DocID against the mapping, persist the
/// short-ID <-> DocID mapping, register `creator` as the document's ACP owner,
/// and record block ownership for the genesis composite, field, and
/// encryption blocks. Returns the parsed DocID.
pub(crate) async fn register_created_doc(
    acp: Option<&Arc<dyn acp::DocumentACP>>,
    creator: Option<&str>,
    systemstore: &NamespaceView,
    datastore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    block_result: &BlockResult,
) -> query::error::Result<DocID> {
    let doc_id_str = &block_result.doc_id;

    if let Some(existing_id) = crate::docid::map::get_doc_id(systemstore, doc_short_id)
        .await
        .map_err(|e| query::error::QueryError::execution(e.to_string()))?
    {
        return Err(query::error::QueryError::execution(format!(
            "Document with ID {existing_id} already exists"
        )));
    }

    let existing = crate::docid::map::get_doc_ref(systemstore, doc_id_str)
        .await
        .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    if let Some(doc_ref) = existing {
        let is_deleted = collection
            .is_deleted(datastore, doc_ref.doc_short_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        if is_deleted {
            return Err(query::error::QueryError::execution(format!(
                "a document with the given ID has been deleted. DocID: {doc_id_str}"
            )));
        }
        return Err(query::error::QueryError::execution(format!(
            "Document with ID {} already exists",
            doc_id_str
        )));
    }

    crate::docid::map::set_doc_id_mapping(
        systemstore,
        collection.resolved_root_id(),
        doc_short_id,
        doc_id_str,
    )
    .await
    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

    // Registration happens only here, on the create branch. A local provider
    // registers in this transaction; an external one cannot join it, so the
    // registration it is owed is recorded here and settled after the commit.
    if let (Some(provider), Some(policy), Some(creator)) =
        (acp, collection.schema().policy.as_ref(), creator)
    {
        let creator = identity::Did::new(creator)
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        let acpstore = systemstore.sibling(storage::namespace::Namespace::Acpstore);
        match provider.bind_transaction(acpstore.clone()) {
            Some(scoped) => scoped
                .register_doc_object(&creator, &policy.id, &policy.resource_name, doc_id_str)
                .await
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            None => acp::PendingRegistrations::new(&acpstore)
                .record(&acp::PendingRegistration::new(
                    creator,
                    &policy.id,
                    &policy.resource_name,
                    doc_id_str,
                ))
                .await
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
        }
    }

    register_block_doc_id_mappings(systemstore, block_result, doc_id_str).await?;
    crate::event::arrivals::record(systemstore, collection.resolved_root_id(), doc_id_str)
        .await
        .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

    DocID::from_string(doc_id_str)
        .map_err(|e| query::error::QueryError::execution(format!("invalid derived DocID: {}", e)))
}

/// Record block ownership (`/d/b/{cid}/{docID}`) for every block produced by
/// a mutation: the composite, each field block, and each encryption block.
pub(crate) async fn register_block_doc_id_mappings(
    systemstore: &NamespaceView,
    block_result: &BlockResult,
    doc_id: &str,
) -> query::error::Result<()> {
    let mut cids = Vec::with_capacity(1 + block_result.field_cids.len());
    cids.push(block_result.cid);
    cids.extend(block_result.field_cids.iter().copied());
    cids.extend(block_result.encryption_cids.iter().copied());
    for cid in cids {
        crate::docid::map::set_block_doc_id_mapping(systemstore, &CidText::new(&cid), doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    }
    Ok(())
}

/// Give each counter this update increments its value after the increment as
/// this transaction sees it: the stored value plus the delta. The increment
/// itself stays a blind operand. The stored value is read without validation:
/// whatever rests on the result validates its own dependency (an index on the
/// counter reads its part with validation, a whole-document put is validated
/// as a write).
async fn set_incremented_counters(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &mut Document,
    doc_short_id: u64,
) -> query::error::Result<()> {
    let increments: Vec<_> = collection
        .schema()
        .fields
        .iter()
        .filter_map(|field| {
            let crdt = crdt::field_crdt(field.crdt_type)?;
            Some((field, crdt, doc.pending_op(&field.name)?.clone()))
        })
        .collect();
    if increments.is_empty() {
        return Ok(());
    }
    let stored = collection
        .read_document_unvalidated(datastore, doc_short_id)
        .await
        .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    for (field, crdt, op) in increments {
        let value = crdt
            .apply(
                field,
                stored.as_ref().and_then(|stored| stored.get(&field.name)),
                &op,
            )
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        doc.set(field.name.clone(), value)
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn ensure_collection_is_active<S: Store>(
    db: &DB<S>,
    collection_name: &str,
    collection: &Collection,
) -> query::error::Result<()> {
    let is_active = db
        .find_collection_by_id(collection.collection_id())
        .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
        .is_some();

    if is_active {
        Ok(())
    } else {
        Err(query::error::QueryError::collection_not_found(
            collection_name,
        ))
    }
}

impl<S: Store + 'static> AutoCommitMutator<S> {
    /// The collection's read guard, then its definition: resolved after the
    /// guard, so a patch or an index committed under the write guard is the
    /// definition this write uses.
    pub(super) async fn guarded_collection(
        &self,
        collection_name: &str,
    ) -> query::error::Result<((), Collection)> {
        let collection = self.get_collection_or_err(collection_name)?;
        ensure_collection_is_active(&self.db, collection_name, &collection)?;
        Ok(((), collection))
    }

    /// Get collection from DB cache or return a not-found error.
    pub(crate) fn get_collection_or_err(
        &self,
        collection_name: &str,
    ) -> query::error::Result<Collection> {
        self.db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))
    }
}

/// Local ownership disappears in the same commit as the deletion marker.
pub(crate) async fn unregister_deleted_doc(
    provider: Option<&Arc<dyn acp::DocumentACP>>,
    systemstore: &NamespaceView,
    collection: &Collection,
    doc_id: &DocID,
) -> query::error::Result<()> {
    if let (Some(provider), Some(policy)) = (provider, collection.schema().policy.as_ref()) {
        let acpstore = systemstore.sibling(storage::namespace::Namespace::Acpstore);
        match provider.bind_transaction(acpstore.clone()) {
            Some(scoped) => scoped
                .unregister_doc_object(&policy.id, &policy.resource_name, &doc_id.to_string())
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?,
            // A registration still owed for a deleted document is owed no more.
            None => acp::PendingRegistrations::new(&acpstore)
                .clear(&policy.id, &policy.resource_name, &doc_id.to_string())
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?,
        }
    }
    Ok(())
}
