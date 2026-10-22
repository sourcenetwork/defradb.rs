//! Document type for DefraDB

use cid::Cid;
use rapidhash::{HashMapExt, RapidHashMap};
use std::sync::Arc;

use schema::{CType, CollectionVersion};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::encoding::{
    canonical_cbor_key_order, cbor_to_normal_value, json_to_normal_value, normal_value_to_cbor,
    normal_value_to_json,
};
use crate::error::{Error, Result};
use crate::field::special::DOC_ID;
use crate::{DocID, FieldOp, FieldValue, NormalValue};

/// A document in DefraDB.
///
/// Documents are the core data type in DefraDB, representing JSON-like objects
/// with fields and values. Each document has:
/// - An optional ID (generated from content if not provided)
/// - A collection of fields with their values
/// - A head CID representing the current state
/// - Dirty tracking for unsaved changes
/// - Schema version tracking for lens migrations
#[derive(Debug, Clone, Serialize)]
pub struct Document {
    /// The document ID (content-addressed)
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<DocID>,

    /// Field values (field name -> FieldValue)
    values: RapidHashMap<String, FieldValue>,

    /// Current head CID (after save)
    #[serde(skip)]
    head: Option<Cid>,

    /// Whether the document has unsaved changes
    #[serde(skip)]
    is_dirty: bool,

    /// The collection definition this document is built against. Field CRDT
    /// types come only from here.
    #[serde(skip)]
    collection: Arc<CollectionVersion>,

    /// The schema version ID this document was stored with.
    ///
    /// This tracks which collection schema version was active when the document
    /// was created or last migrated. Used by the lens system to determine if
    /// a document needs migration to match the current collection version.
    ///
    /// Stored in the datastore at key `/{collectionShortID}/v/{docID}/v`.
    #[serde(skip)]
    schema_version_id: Option<String>,

    /// Whether this document has been soft-deleted (marked with DeletedObjectMarker).
    #[serde(skip)]
    deleted: bool,

    /// Operations a write applies on top of the field's stored state, keyed by
    /// field. The block builder needs the operation itself (a counter's
    /// increment), not the value it materializes to.
    #[serde(skip)]
    pending_ops: RapidHashMap<String, FieldOp>,

    #[serde(skip)]
    write_preparation: Option<std::sync::Arc<crate::WritePreparation>>,
}

impl Document {
    /// Precomputed storage identity and keys for this write, if any.
    pub fn write_preparation(&self) -> Option<&std::sync::Arc<crate::WritePreparation>> {
        self.write_preparation.as_ref()
    }

    /// Attach transient write inputs without changing document values or dirty fields.
    pub fn set_write_preparation(&mut self, preparation: std::sync::Arc<crate::WritePreparation>) {
        self.write_preparation = Some(preparation);
    }

    pub fn clear_write_preparation(&mut self) {
        self.write_preparation = None;
    }

    /// Create a new empty document for a collection.
    pub fn new(collection: impl Into<Arc<CollectionVersion>>) -> Self {
        Self {
            id: None,
            values: RapidHashMap::new(),
            head: None,
            is_dirty: true,
            collection: collection.into(),
            schema_version_id: None,
            deleted: false,
            pending_ops: RapidHashMap::new(),
            write_preparation: None,
        }
    }

    /// Create a new document with a specific ID.
    pub fn with_id(id: DocID, collection: impl Into<Arc<CollectionVersion>>) -> Self {
        let mut doc = Self::new(collection);
        doc.id = Some(id);
        doc
    }

    /// Create a document from a JSON byte slice.
    pub fn from_json(json: &[u8], collection: impl Into<Arc<CollectionVersion>>) -> Result<Self> {
        let map: RapidHashMap<String, serde_json::Value> = serde_json::from_slice(json)?;
        Self::from_map(map, collection)
    }

    /// Create a document from a JSON string.
    pub fn from_json_str(
        json: &str,
        collection: impl Into<Arc<CollectionVersion>>,
    ) -> Result<Self> {
        Self::from_json(json.as_bytes(), collection)
    }

    /// Create a document from a map of values.
    pub fn from_map(
        mut map: RapidHashMap<String, serde_json::Value>,
        collection: impl Into<Arc<CollectionVersion>>,
    ) -> Result<Self> {
        let mut doc = Document::new(collection);

        if let Some(id_value) = map.remove(DOC_ID) {
            if let Some(id_str) = id_value.as_str() {
                doc.id = Some(DocID::from_string(id_str)?);
            } else {
                return Err(Error::InvalidFieldValue {
                    field: DOC_ID.to_string(),
                    message: format!("_docID must be a string, got: {}", id_value),
                });
            }
        }

        for (key, value) in map {
            doc.set(key, json_to_normal_value(value)?)?;
        }

        Ok(doc)
    }

    /// Get the document ID.
    pub fn id(&self) -> Option<&DocID> {
        self.id.as_ref()
    }

    /// Set the document ID.
    pub fn set_id(&mut self, id: DocID) {
        self.id = Some(id);
    }

    /// Get the current head CID.
    pub fn head(&self) -> Option<&Cid> {
        self.head.as_ref()
    }

    /// Set the head CID.
    pub fn set_head(&mut self, cid: Cid) {
        self.head = Some(cid);
    }

    /// Get the collection definition.
    pub fn collection(&self) -> &Arc<CollectionVersion> {
        &self.collection
    }

    /// The CRDT type the collection definition gives a field.
    pub fn crdt_type(&self, field: &str) -> Option<CType> {
        self.collection.field_by_name(field).map(|f| f.crdt_type)
    }

    /// Get the schema version ID this document was stored with.
    ///
    /// This is used by the lens migration system to determine if a document
    /// needs to be transformed to match the current collection schema version.
    pub fn schema_version_id(&self) -> Option<&str> {
        self.schema_version_id.as_deref()
    }

    /// Set the schema version ID.
    ///
    /// This should be called when:
    /// - Creating a new document (set to current collection version)
    /// - Loading a document from storage (set to stored version)
    /// - After migrating a document (set to target version)
    pub fn set_schema_version_id(&mut self, version_id: impl Into<String>) {
        self.schema_version_id = Some(version_id.into());
    }

    /// Check if this document needs migration to the target schema version.
    ///
    /// Returns `true` if the document has a schema version that differs from
    /// the target, indicating lens transforms should be applied.
    pub fn needs_migration(&self, target_version_id: &str) -> bool {
        match &self.schema_version_id {
            Some(version) => version != target_version_id,
            // If no version is set, assume it's already at target (legacy behavior)
            None => false,
        }
    }

    /// Check if this document is soft-deleted.
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    /// Mark this document as soft-deleted.
    pub fn set_deleted(&mut self, deleted: bool) {
        self.deleted = deleted;
    }

    /// Check if the document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.is_dirty || self.values.values().any(|v| v.is_dirty())
    }

    /// Mark the document as clean (saved).
    pub fn clean(&mut self) {
        self.is_dirty = false;
        for value in self.values.values_mut() {
            value.clean();
        }
    }

    /// Mark the document as dirty.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Get a field value by name.
    pub fn get(&self, field: &str) -> Option<&NormalValue> {
        self.values.get(field).map(|fv| fv.value())
    }

    /// Get a field value wrapper by name.
    pub fn get_field_value(&self, field: &str) -> Option<&FieldValue> {
        self.values.get(field)
    }

    /// Get a mutable field value by name.
    ///
    /// Only marks the document dirty when the requested field exists.
    /// Returning `None` for a missing field must not trigger a save or
    /// CRDT delta — matches Go's `document.go`, which sets the dirty flag
    /// inside `doc.set()` only when a value is actually written.
    pub fn get_mut(&mut self, field: &str) -> Option<&mut FieldValue> {
        let value = self.values.get_mut(field)?;
        self.is_dirty = true;
        Some(value)
    }

    /// Set a field value. The field must exist in the collection definition,
    /// which also gives the field its CRDT type.
    pub fn set(&mut self, field: impl Into<String>, value: impl Into<NormalValue>) -> Result<()> {
        let field_name = field.into();
        let crdt_type = self
            .crdt_type(&field_name)
            .ok_or_else(|| Error::FieldNotExist(field_name.clone()))?;
        let field_value = FieldValue::new(crdt_type, value.into())?;
        self.values.insert(field_name, field_value);
        self.is_dirty = true;
        Ok(())
    }

    /// Insert a value decoded from storage. Reads never fail on a field the
    /// current definition lacks (data written under an older schema version);
    /// such a field is kept by name with no CRDT type.
    pub fn load(&mut self, field: String, value: NormalValue) {
        let crdt_type = self.crdt_type(&field).unwrap_or(CType::None);
        self.values
            .insert(field, FieldValue::loaded(crdt_type, value));
    }

    /// Set a value the definition has not vouched for, as a lens or a
    /// migration produces it: kept dirty, never checked against the definition.
    pub fn set_unchecked(&mut self, field: String, value: NormalValue) {
        self.load(field.clone(), value);
        if let Some(fv) = self.values.get_mut(&field) {
            fv.mark_dirty();
        }
        self.is_dirty = true;
    }

    /// Remove a field from the document.
    pub fn remove(&mut self, field: &str) -> Option<FieldValue> {
        let result = self.values.remove(field);
        if result.is_some() {
            self.is_dirty = true;
        }
        result
    }

    /// Get all field names.
    pub fn field_names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(|s| s.as_str())
    }

    /// Get all values.
    pub fn values(&self) -> &RapidHashMap<String, FieldValue> {
        &self.values
    }

    /// Express an operation on a field. The field must exist in the collection
    /// definition. `Set` writes the value, as [`Document::set`] does. Any other
    /// operation is kept as the field's pending operation and leaves its value
    /// alone: the caller holds the type's arithmetic and sets the value the
    /// operation materializes to.
    pub fn edit(&mut self, field: impl Into<String>, op: FieldOp) -> Result<()> {
        let field = field.into();
        match op {
            FieldOp::Set(value) => self.set(field, value),
            op => {
                if self.crdt_type(&field).is_none() {
                    return Err(Error::FieldNotExist(field));
                }
                self.pending_ops.insert(field, op);
                self.is_dirty = true;
                Ok(())
            }
        }
    }

    /// The operation this write applies to a field on top of its stored state.
    pub fn pending_op(&self, field: &str) -> Option<&FieldOp> {
        self.pending_ops.get(field)
    }

    /// Get all dirty fields.
    pub fn dirty_fields(&self) -> impl Iterator<Item = (&str, &FieldValue)> {
        self.values
            .iter()
            .filter(|(_, v)| v.is_dirty())
            .map(|(k, v)| (k.as_str(), v))
    }

    /// Convert the document to a map of values.
    ///
    /// Returns an error if any field contains non-finite floats (NaN, Infinity),
    /// matching Go's encoding/json behavior.
    pub fn to_map(&self) -> Result<RapidHashMap<String, serde_json::Value>> {
        let mut map = RapidHashMap::new();

        if let Some(ref id) = self.id {
            map.insert(
                DOC_ID.to_string(),
                serde_json::Value::String(id.to_string()),
            );
        }

        for (key, field_value) in &self.values {
            map.insert(key.clone(), normal_value_to_json(field_value.value())?);
        }

        Ok(map)
    }

    /// Encode the document to CBOR bytes.
    ///
    /// This encodes only the values, not the metadata (id, head, dirty flag).
    /// Uses canonical CBOR ordering (RFC 7049 Section 3.9) for Go compatibility:
    /// - Keys sorted by length first (shorter keys first)
    /// - Then lexicographically (bytewise) within same length
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        // Collect keys and sort using canonical CBOR ordering
        let mut keys: Vec<&str> = self.values.keys().map(|k| k.as_str()).collect();
        keys.sort_by(canonical_cbor_key_order);

        // Build CBOR map using ciborium::Value for proper map encoding.
        // Skip null values to match Go's toMap(true) behavior: setting a
        // property to nil produces the same serialization as omitting it,
        // which is critical for deterministic docID generation.
        let mut map_entries: Vec<(ciborium::Value, ciborium::Value)> =
            Vec::with_capacity(keys.len());
        for k in keys {
            let field_value = self
                .values
                .get(k)
                .ok_or_else(|| Error::FieldNotFound(k.to_string()))?;
            if field_value.value().is_nil() {
                continue;
            }
            let key = ciborium::Value::Text(k.to_string());
            let value = normal_value_to_cbor(field_value.value())?;
            map_entries.push((key, value));
        }

        let cbor_map = ciborium::Value::Map(map_entries);

        let mut buf = Vec::new();
        ciborium::into_writer(&cbor_map, &mut buf).map_err(|e| Error::CborEncode(e.to_string()))?;
        Ok(buf)
    }

    /// Decode a document from CBOR bytes.
    ///
    /// This decodes only the values. The document will have no ID, head, or collection set.
    /// The document is marked as clean (not dirty) since it was loaded from storage.
    pub fn from_cbor(bytes: &[u8], collection: impl Into<Arc<CollectionVersion>>) -> Result<Self> {
        let cbor_map: ciborium::Value =
            ciborium::from_reader(bytes).map_err(|e| Error::CborDecode(e.to_string()))?;

        let map = match cbor_map {
            ciborium::Value::Map(m) => m,
            _ => return Err(Error::CborDecode("expected CBOR map".into())),
        };

        let mut doc = Document::new(collection);
        doc.is_dirty = false; // Loaded from storage, not dirty

        for (k, v) in map {
            let key = match k {
                ciborium::Value::Text(s) => s,
                _ => return Err(Error::CborDecode("map key must be text".into())),
            };

            let normal_value = cbor_to_normal_value(v)?;
            doc.load(key, normal_value);
        }

        doc.clean();
        Ok(doc)
    }

    /// Check if the document has a specific field.
    pub fn has_field(&self, field: &str) -> bool {
        self.values.contains_key(field)
    }

    /// Get the number of fields in the document.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the document has no fields.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D>(_: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Err(de::Error::custom(
            "a document is built against its collection definition and cannot be deserialized without one",
        ))
    }
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.values == other.values
    }
}
