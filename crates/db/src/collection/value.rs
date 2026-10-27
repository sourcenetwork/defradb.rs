//! Materialized document puts and field operands. Block CBOR stays unchanged.
use super::*;
use crdt::stored::{StoredDocument, StoredField};
use document::FieldOp;
use rapidhash::RapidHashMap;
use storage::namespace::Namespace;

pub(crate) struct FieldOrder {
    pub(crate) priority: u64,
    pub(crate) raw_value: Option<Vec<u8>>,
}
type Priorities = RapidHashMap<String, FieldOrder>;

fn field_crdt<'crdts>(
    crdts: &'crdts crdt::Crdts,
    field: &schema::FieldDescription,
) -> Result<crdt::CrdtRef<'crdts>> {
    crdts.field(field.crdt_type).ok_or_else(|| {
        Error::InvalidDocument(format!(
            "no CRDT for {} field {}",
            field.crdt_type, field.name
        ))
    })
}

/// The state `field` stores at `priority`: the merged block bytes when a merge
/// resolved them, else the state of `value`.
fn field_value(
    crdts: &crdt::Crdts,
    field: &schema::FieldDescription,
    value: &NormalValue,
    priority: u64,
    raw: Option<&[u8]>,
) -> Result<StoredField> {
    let crdt = field_crdt(crdts, field)?;
    match raw {
        Some(raw) => crdt.block_state(field, priority, raw),
        None => crdt.state(field, priority, value),
    }
    .map_err(|error| Error::InvalidDocument(error.to_string()))
}

/// The part a tagged field's put stores: `base`, the part already stored,
/// after the write's prepared operand. Without one, the part stands as it is,
/// or, with none stored, is the state of the field's value. `None` for a nil
/// value with nothing stored.
fn tagged_put(
    tagged: &dyn crdt::TaggedCrdt,
    field: &schema::FieldDescription,
    doc: &Document,
    priority: u64,
    base: Option<&StoredField>,
) -> Result<Option<StoredField>> {
    let invalid = |e: &dyn std::fmt::Display| Error::InvalidDocument(e.to_string());
    let base = base.filter(|part| part.name == field.name);
    let recorded = match base {
        Some(base) => !tagged
            .tags(field, base.part())
            .map_err(|e| invalid(&e))?
            .is_empty(),
        None => false,
    };
    let mut part = match (doc.pending_op(&field.name), base) {
        (Some(FieldOp::Operand { operand, .. }), base) => {
            let operand = crdt::field::tagged::decode_operand(operand)
                .ok_or_else(|| invalid(&"invalid prepared operand"))?;
            crdt::field::tagged::land(tagged, base, operand)
                .ok_or_else(|| invalid(&"a prepared operand does not land"))?
        }
        (_, Some(base)) if recorded => base.clone(),
        _ => {
            let value = doc.get(&field.name).unwrap_or(&NormalValue::Null);
            if value.is_nil() {
                return Ok(None);
            }
            tagged
                .state(field, priority, value)
                .map_err(|e| invalid(&e))?
        }
    };
    part.priority = priority;
    Ok(Some(part))
}

impl Collection {
    pub(crate) async fn put_document_value(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc: &Document,
        creating: bool,
        priorities: Option<&Priorities>,
    ) -> Result<()> {
        let Some(ids) = self.part_ids(datastore).await? else {
            // Low-level collection fixtures may bypass definition installation.
            // Such values retain whole-value validation and never receive operands.
            return datastore
                .set(&self.doc_key(doc_short_id), &doc.to_cbor()?)
                .await
                .map_err(Error::Storage);
        };
        let heads = if creating {
            None
        } else {
            Some(
                crate::block::builder::DocHeadsSnapshot::load(
                    &datastore.sibling(Namespace::Headstore),
                    doc_short_id,
                )
                .await
                .map_err(Error::InvalidDocument)?,
            )
        };
        let previous = if !creating
            && self.def.fields.iter().any(|field| {
                self.crdts
                    .field(field.crdt_type)
                    .is_some_and(|crdt| crdt.tagged().is_some())
            }) {
            datastore
                .get_parts(&self.doc_key(doc_short_id), &[])
                .await
                .map_err(Error::Storage)?
                .as_deref()
                .and_then(StoredDocument::decode)
        } else {
            None
        };
        let mut stored = StoredDocument::default();
        for field in &self.def.fields {
            let value = doc.get(&field.name).unwrap_or(&NormalValue::Null);
            let Some(crdt) = self.crdts.field(field.crdt_type) else {
                continue;
            };
            if value.is_nil() && self.crdts.is_unregistered(field.crdt_type) {
                continue;
            }
            let id = ids.get(&field.name).ok_or_else(|| {
                Error::InvalidDocument(format!("missing part ID for {}", field.name))
            })?;
            let priority_of = |field: &schema::FieldDescription| {
                priorities
                    .and_then(|values| values.get(&field.name).map(|order| order.priority))
                    .unwrap_or_else(|| {
                        heads
                            .as_ref()
                            .map(|heads| heads.field_max_priority(&field.name))
                            .unwrap_or(1)
                    })
            };
            if let Some(tagged) = crdt.tagged() {
                let base = previous
                    .as_ref()
                    .and_then(|previous| previous.fields.get(id));
                if let Some(part) = tagged_put(tagged, field, doc, priority_of(field), base)? {
                    stored.fields.insert(*id, part);
                }
                continue;
            }
            if value.is_nil() && crdt.replicates_ops() {
                continue;
            }
            let priority = priority_of(field);
            let raw = priorities
                .and_then(|values| values.get(&field.name))
                .and_then(|order| order.raw_value.as_deref());
            stored
                .fields
                .insert(*id, field_value(&self.crdts, field, value, priority, raw)?);
        }
        let bytes = stored
            .encode()
            .ok_or_else(|| Error::InvalidDocument("invalid local document value".into()))?;
        let crdts =
            crate::database::stored_crdts::custom(stored.fields.values().map(|field| field.kind));
        crate::database::stored_crdts::record(datastore, crdts)
            .await
            .map_err(Error::Storage)?;
        datastore
            .set(&self.doc_key(doc_short_id), &bytes)
            .await
            .map_err(Error::Storage)
    }

    /// Materialize CRDT-resolved values through operands when the schema is
    /// unchanged. Integer mirror differences are exact modulo 2^64. Floating
    /// point subtraction cannot reconstruct an exact delta, so those repairs
    /// retain a validated put instead of silently changing rounding.
    pub(crate) async fn merge_resolved_document_value(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc: &Document,
        priorities: &Priorities,
    ) -> Result<bool> {
        if self.load_version(datastore, doc_short_id).await?.as_deref() != Some(self.version_id()) {
            return Ok(false);
        }
        let Some(ids) = self.part_ids(datastore).await? else {
            return Ok(false);
        };
        // Operations reach the value as their own operands, so a resolved
        // document never restates them and this read decides nothing: it only
        // checks part identities.
        let key = self.doc_key(doc_short_id);
        let Some(bytes) = datastore
            .get_parts(&key, &[])
            .await
            .map_err(Error::Storage)?
        else {
            return Ok(false);
        };
        let Some(stored) = StoredDocument::decode(&bytes) else {
            return Ok(false);
        };
        let mut parts = StoredDocument::default();
        for field in &self.def.fields {
            // An operation reaches the value as its own operand, except a
            // causal one, whose merged value is given whole.
            if self
                .crdts
                .field(field.crdt_type)
                .is_none_or(|crdt| crdt.replicates_ops() && crdt.causal().is_none())
            {
                continue;
            }
            let Some(order) = priorities.get(&field.name) else {
                continue;
            };
            let Some(id) = ids.get(&field.name) else {
                return Ok(false);
            };
            let old = stored.fields.get(id);
            let value = doc.get(&field.name).unwrap_or(&NormalValue::Null);
            let resolved = field_value(
                &self.crdts,
                field,
                value,
                order.priority,
                order.raw_value.as_deref(),
            )?;
            if old.is_some_and(|old| old.name != resolved.name || old.kind != resolved.kind) {
                return Ok(false);
            }
            parts.fields.insert(*id, resolved);
        }
        if !parts.fields.is_empty() {
            let operand = storage::merge::document_parts(&parts).ok_or_else(|| {
                Error::InvalidDocument("invalid resolved document operand".into())
            })?;
            let crdts = crate::database::stored_crdts::custom(
                parts.fields.values().map(|field| field.kind),
            );
            crate::database::stored_crdts::record(datastore, crdts)
                .await
                .map_err(Error::Storage)?;
            datastore
                .merge(&key, &operand)
                .await
                .map_err(Error::Storage)?;
        }
        Ok(true)
    }

    /// The document as this transaction sees it, read without validation.
    /// Only for values no decision rests on, such as the result of a blind
    /// counter increment.
    pub(crate) async fn read_document_unvalidated(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
    ) -> Result<Option<Document>> {
        datastore
            .get_parts(&self.doc_key(doc_short_id), &[])
            .await
            .map_err(Error::Storage)?
            .map(|bytes| {
                crdt::stored::document_from_storage(&bytes, self.schema_arc(), &self.crdts)
                    .map_err(Error::from)
            })
            .transpose()
    }

    /// The stored state of `field`, read without validation: for a tagged
    /// field, whose writes and merges decide nothing on it.
    pub(crate) async fn read_part_unvalidated(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        field: &str,
    ) -> Result<Option<StoredField>> {
        Ok(datastore
            .get_parts(&self.doc_key(doc_short_id), &[])
            .await
            .map_err(Error::Storage)?
            .as_deref()
            .and_then(StoredDocument::decode)
            .and_then(|stored| stored.fields.into_values().find(|part| part.name == field)))
    }

    /// Write a local update's changed fields as one operand, each at the
    /// priority its new block takes: one above the field's heads in `heads`,
    /// the snapshot the update builds its blocks from.
    pub(crate) async fn merge_local_document_value(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc: &Document,
        modified: &rapidhash::RapidHashSet<String>,
        heads: &crate::block::builder::DocHeadsSnapshot,
    ) -> Result<()> {
        let key = self.doc_key(doc_short_id);
        let ids = self.part_ids(datastore).await?;
        let bytes = datastore
            .get_parts(&key, &[])
            .await
            .map_err(Error::Storage)?;
        let stored = bytes.as_deref().and_then(StoredDocument::decode);
        let priorities: Priorities = modified
            .iter()
            .map(|name| {
                Ok((
                    name.clone(),
                    FieldOrder {
                        priority: heads.field_max_priority(name).checked_add(1).ok_or_else(
                            || Error::InvalidDocument("field priority overflow".into()),
                        )?,
                        raw_value: None,
                    },
                ))
            })
            .collect::<Result<_>>()?;
        let (Some(ids), Some(stored)) = (ids, stored) else {
            return self
                .put_document_value(datastore, doc_short_id, doc, false, Some(&priorities))
                .await;
        };
        if self.load_version(datastore, doc_short_id).await?.as_deref() != Some(self.version_id()) {
            return self
                .put_document_value(datastore, doc_short_id, doc, false, Some(&priorities))
                .await;
        }
        let mut parts = StoredDocument::default();
        for field in &self.def.fields {
            if !modified.contains(&field.name) {
                continue;
            }
            let id = ids.get(&field.name).ok_or_else(|| {
                Error::InvalidDocument(format!("missing part ID for {}", field.name))
            })?;
            let priority = priorities[&field.name].priority;
            if let Some(FieldOp::Operand { operand, .. }) = doc.pending_op(&field.name) {
                let mut part = crdt::field::tagged::decode_operand(operand)
                    .ok_or_else(|| Error::InvalidDocument("invalid prepared operand".into()))?;
                part.priority = priority;
                parts.fields.insert(*id, part);
                continue;
            }
            let current = doc.get(&field.name).unwrap_or(&NormalValue::Null);
            let write = field_crdt(&self.crdts, field)?.write(current, doc.pending_op(&field.name));
            if !write.merges {
                // A write with no operand form, such as an absolute counter
                // replacement, is structural: the document value is put whole.
                return self
                    .put_document_value(datastore, doc_short_id, doc, false, Some(&priorities))
                    .await;
            }
            parts.fields.insert(
                *id,
                field_value(&self.crdts, field, write.data, priority, None)?,
            );
        }
        // An operand must name each part as the stored value does; a schema
        // transition is a put above, never an operand.
        let mut projected = stored;
        projected.apply(parts.clone(), &self.crdts).ok_or_else(|| {
            Error::InvalidDocument(
                "document part identity changed without a schema transition".into(),
            )
        })?;
        if !parts.fields.is_empty() {
            let operand = storage::merge::document_parts(&parts)
                .ok_or_else(|| Error::InvalidDocument("invalid document operand".into()))?;
            let crdts = crate::database::stored_crdts::custom(
                parts.fields.values().map(|field| field.kind),
            );
            crate::database::stored_crdts::record(datastore, crdts)
                .await
                .map_err(Error::Storage)?;
            datastore
                .merge(&key, &operand)
                .await
                .map_err(Error::Storage)?;
        }
        Ok(())
    }
}
