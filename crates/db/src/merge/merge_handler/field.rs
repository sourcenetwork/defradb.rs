//! One field block merged into its document, whichever CRDT wrote it. A state
//! joins the field's register ([`field_state`](super::field_state)); an
//! operation applies to the document value exactly once
//! ([`field_ops`](super::field_ops)).
use super::*;
use crate::collection::value::FieldOrder;

/// What merging one field block did.
pub struct FieldMergeResult {
    pub(crate) applied: bool,
    /// The field's merged value, for document reconstruction.
    pub(crate) value: Option<NormalValue>,
    /// A state's merged priority and bytes, which the document value stores as
    /// they are; `None` for an operation, which reached the document value as
    /// its own operand.
    pub(crate) order: Option<FieldOrder>,
}

impl FieldMergeResult {
    /// True when the block changed the field rather than lost or repeated.
    pub fn applied(&self) -> bool {
        self.applied
    }

    /// The field's value after the merge, as the merging transaction sees it.
    pub fn value(&self) -> Option<&NormalValue> {
        self.value.as_ref()
    }

    /// Whether the block carried an operation, which the blockstore must
    /// record as merged so no ingest path applies it again.
    pub(crate) fn replicated_op(&self) -> bool {
        self.order.is_none()
    }
}

/// The declaration a field the block's collection does not declare merges
/// under: an untyped field of the CRDT that wrote the block.
fn undeclared(name: &str, ctype: CType) -> FieldDescription {
    FieldDescription::new("", name, FieldKind::Scalar(ScalarKind::None), ctype)
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Merge one standalone field block within the caller's transaction,
    /// through the CRDT that wrote it.
    #[allow(clippy::too_many_arguments)]
    pub async fn merge_field_in_txn(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        delta: &CrdtDelta,
        fallback_collection_id: Option<&str>,
        doc_id_str: &str,
        doc_short_id: u64,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        self.merge_field_delta_in_txn(
            datastore,
            headstore,
            cid,
            delta,
            fallback_collection_id,
            doc_id_str,
            doc_short_id,
            &mut super::block_reads::BlockReads::default(),
        )
        .await
    }

    /// [`merge_field_in_txn`](Self::merge_field_in_txn) for a block of a
    /// composite merge, sharing its block reads.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn merge_field_delta_in_txn(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        delta: &CrdtDelta,
        fallback_collection_id: Option<&str>,
        doc_id_str: &str,
        doc_short_id: u64,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let Some(field_delta) = delta.field() else {
            return Err(MergeError::UnsupportedDelta(format!(
                "not a field delta: {:?}",
                std::mem::discriminant(delta)
            )));
        };
        tracing::debug!(
            cid = %cid,
            field_name = %field_delta.field_name,
            priority = field_delta.priority,
            "Merging field delta in transaction"
        );
        let collection = self
            .block_collection(field_delta.schema_version_id, fallback_collection_id)
            .await?;
        let declared = collection
            .as_ref()
            .and_then(|collection| collection.schema().field_by_name(field_delta.field_name));
        let crdt = crdt::delta_crdt(delta, declared.map_or(CType::None, |field| field.crdt_type))
            .ok_or_else(|| {
            MergeError::UnsupportedDelta("no CRDT wrote this field block".into())
        })?;

        if !crdt.replicates_ops() {
            let undeclared_field;
            let field = match declared {
                Some(field) => field,
                None => {
                    undeclared_field = undeclared(field_delta.field_name, crdt.ctype());
                    &undeclared_field
                }
            };
            return self
                .merge_field_state(
                    datastore,
                    headstore,
                    crdt,
                    field,
                    &field_delta,
                    collection.as_ref(),
                    doc_id_str,
                    doc_short_id,
                    reads,
                )
                .await;
        }

        // An operation is read as its field's kind declares it.
        let collection = collection.as_ref().ok_or_else(|| {
            MergeError::MissingMetadata(format!(
                "Collection not found for schema_version_id: {}",
                field_delta.schema_version_id
            ))
        })?;
        let field = declared.ok_or_else(|| {
            MergeError::MissingMetadata(format!(
                "Field '{}' not found in collection",
                field_delta.field_name
            ))
        })?;
        self.apply_field_op_once(
            datastore,
            headstore,
            cid,
            crdt,
            collection,
            field,
            &field_delta,
            doc_short_id,
        )
        .await
    }
}
