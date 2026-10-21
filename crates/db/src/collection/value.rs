//! Materialized document puts and field operands. Block CBOR stays unchanged.
use super::*;
use document::stored::{StoredDocument, StoredField, ValueKind};
use rapidhash::RapidHashMap;
use storage::namespace::Namespace;

pub(crate) struct FieldOrder {
    pub(crate) priority: u64,
    pub(crate) raw_value: Option<Vec<u8>>,
}
type Priorities = RapidHashMap<String, FieldOrder>;

/// How the document value stores a counter field, or `None` when the field is
/// not a counter of a numeric kind.
pub(crate) fn counter_kind(field: &schema::FieldDescription) -> Option<ValueKind> {
    if !field.crdt_type.is_counter() {
        return None;
    }
    match field.kind.as_scalar().map(ScalarKind::base_kind) {
        Some(ScalarKind::Int) => Some(ValueKind::Int64),
        Some(ScalarKind::Float32) => Some(ValueKind::Float32),
        Some(ScalarKind::Float64) => Some(ValueKind::Float64),
        _ => None,
    }
}

fn field_value(
    field: &schema::FieldDescription,
    value: &NormalValue,
    priority: u64,
) -> Result<StoredField> {
    if field.crdt_type.is_text() {
        return StoredField::text(field.name.clone(), priority, value).map_err(Error::from);
    }
    if !field.crdt_type.is_counter() {
        return StoredField::lww(field.name.clone(), priority, value).map_err(Error::from);
    }
    let kind = counter_kind(field).ok_or_else(|| {
        Error::InvalidDocument(format!("unsupported counter field {}", field.name))
    })?;
    StoredField::counter(field.name.clone(), kind, priority, value).map_err(Error::from)
}

/// The Json part a put stores: `base`, the part already stored, after the
/// write's operand. A write without one (a create, or a fixture) stores its
/// value as one root block over an absent field, the block a create builds.
fn json_put(
    field: &schema::FieldDescription,
    doc: &Document,
    priority: u64,
    base: Option<&StoredField>,
) -> Result<StoredField> {
    use defra_core::json_crdt::{self, Frontier};
    let mut frontier = match base.filter(|part| part.kind == ValueKind::Json) {
        Some(part) => {
            Frontier::decode(&part.value).map_err(|e| Error::InvalidDocument(e.to_string()))?
        }
        None => Frontier::default(),
    };
    let delta = match doc.json_delta(&field.name) {
        Some(delta) => delta.clone(),
        None if frontier.entries.is_empty() => {
            let ops = doc.json_ops_or_value(&field.name);
            let data =
                json_crdt::encode_ops(&ops).map_err(|e| Error::InvalidDocument(e.to_string()))?;
            frontier.local_delta(&ops, &json_crdt::block_id(priority, &[], &data), priority)
        }
        None => Frontier::default(),
    };
    frontier.compose(delta);
    frontier.settle();
    StoredField::json(field.name.clone(), priority, &frontier).map_err(Error::from)
}

/// The operand of a Json part: the one the write recorded.
fn json_operand(
    field: &schema::FieldDescription,
    doc: &Document,
    priority: u64,
) -> Result<Option<StoredField>> {
    doc.json_delta(&field.name)
        .map(|delta| StoredField::json(field.name.clone(), priority, delta).map_err(Error::from))
        .transpose()
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
        let previous = if !creating && self.def.fields.iter().any(|f| f.crdt_type.is_json()) {
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
            if field.crdt_type.is_counter() && value.is_nil() {
                continue;
            }
            let id = ids.get(&field.name).ok_or_else(|| {
                Error::InvalidDocument(format!("missing part ID for {}", field.name))
            })?;
            let priority = priorities
                .and_then(|values| values.get(&field.name).map(|order| order.priority))
                .unwrap_or_else(|| {
                    heads
                        .as_ref()
                        .map(|heads| heads.field_max_priority(&field.name))
                        .unwrap_or(1)
                });
            if field.crdt_type.is_json() {
                let base = previous
                    .as_ref()
                    .and_then(|previous| previous.fields.get(id));
                stored
                    .fields
                    .insert(*id, json_put(field, doc, priority, base)?);
                continue;
            }
            let mut stored_field = field_value(field, value, priority)?;
            if stored_field.kind == ValueKind::Lww {
                if let Some(raw) = priorities
                    .and_then(|values| values.get(&field.name))
                    .and_then(|order| order.raw_value.as_ref())
                {
                    stored_field.value.clone_from(raw);
                }
            }
            stored.fields.insert(*id, stored_field);
        }
        let bytes = stored
            .encode()
            .ok_or_else(|| Error::InvalidDocument("invalid local document value".into()))?;
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
        // Counter increments reach the value as their own operands
        // (`crdt::Counter`), so a resolved document never restates a counter
        // and this read decides nothing: it only checks part identities.
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
            if field.crdt_type.is_counter() {
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
            let mut resolved = if field.crdt_type.is_json() {
                match json_operand(field, doc, order.priority)? {
                    Some(operand) => operand,
                    None => continue,
                }
            } else {
                field_value(field, value, order.priority)?
            };
            if old.is_some_and(|old| old.name != resolved.name || old.kind != resolved.kind) {
                return Ok(false);
            }
            if let Some(raw) = order.raw_value.as_ref() {
                resolved.value.clone_from(raw);
            }
            parts.fields.insert(*id, resolved);
        }
        if !parts.fields.is_empty() {
            let operand = storage::merge::document_parts(&parts).ok_or_else(|| {
                Error::InvalidDocument("invalid resolved document operand".into())
            })?;
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
            .map(|bytes| Document::from_storage_cbor(&bytes).map_err(Error::from))
            .transpose()
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
            let value = if field.crdt_type.is_counter() {
                let Some(delta) = doc.get_counter_delta(&field.name) else {
                    // Absolute counter replacement is structural, not an increment.
                    return self
                        .put_document_value(datastore, doc_short_id, doc, false, Some(&priorities))
                        .await;
                };
                delta
            } else {
                doc.get(&field.name).unwrap_or(&NormalValue::Null)
            };
            let id = ids.get(&field.name).ok_or_else(|| {
                Error::InvalidDocument(format!("missing part ID for {}", field.name))
            })?;
            let priority = priorities[&field.name].priority;
            let part = if field.crdt_type.is_json() {
                match json_operand(field, doc, priority)? {
                    Some(operand) => operand,
                    None => {
                        return self
                            .put_document_value(
                                datastore,
                                doc_short_id,
                                doc,
                                false,
                                Some(&priorities),
                            )
                            .await
                    }
                }
            } else {
                field_value(field, value, priority)?
            };
            parts.fields.insert(*id, part);
        }
        // An operand must name each part as the stored value does; a schema
        // transition is a put above, never an operand.
        let mut projected = stored;
        projected.apply(parts.clone()).ok_or_else(|| {
            Error::InvalidDocument(
                "document part identity changed without a schema transition".into(),
            )
        })?;
        if !parts.fields.is_empty() {
            let operand = storage::merge::document_parts(&parts)
                .ok_or_else(|| Error::InvalidDocument("invalid document operand".into()))?;
            datastore
                .merge(&key, &operand)
                .await
                .map_err(Error::Storage)?;
        }
        Ok(())
    }
}

impl Collection {
    /// The stored frontier of the Json field `field`, read without
    /// validation, empty when the document has none. A writer's operand
    /// removes only the entries this read returns, which are exactly the
    /// ones under the heads it read in the same snapshot, so a concurrent
    /// writer's entry survives and nothing needs validating.
    pub(crate) async fn read_json_frontier(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        field: &str,
    ) -> Result<defra_core::json_crdt::Frontier> {
        let bytes = datastore
            .get_parts(&self.doc_key(doc_short_id), &[])
            .await
            .map_err(Error::Storage)?;
        let part = bytes
            .as_deref()
            .and_then(StoredDocument::decode)
            .and_then(|stored| {
                stored
                    .fields
                    .into_values()
                    .find(|part| part.name == field && part.kind == ValueKind::Json)
            });
        match part {
            Some(part) => defra_core::json_crdt::Frontier::decode(&part.value)
                .map_err(|error| Error::InvalidDocument(error.to_string())),
            None => Ok(defra_core::json_crdt::Frontier::default()),
        }
    }
}
