//! The field-mapping file: Go Shinzo's `kvFieldMappingFile`, which tells an
//! importing node which collection and field each short ID in a snapshot
//! names. `documents` is defradb.rs's own member: each exported document's
//! short ID and DocID. Go's decoder ignores members it does not know.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMapping {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "collectionShortID")]
    pub collection_short_id: u32,
    #[serde(default)]
    pub fields: Vec<FieldEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<DocumentEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldEntry {
    pub name: String,
    #[serde(rename = "fieldID")]
    pub field_id: String,
    #[serde(rename = "shortID")]
    pub short_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEntry {
    #[serde(rename = "docID")]
    pub doc_id: String,
    #[serde(rename = "shortID")]
    pub short_id: u64,
}

/// The snapshot short ID of each field of `version` that has a field ID, in
/// the version's field order, numbered from 1 as Go numbers a new collection's
/// fields. defradb.rs keeps no per-node field short IDs, so a snapshot names
/// fields by these and its mapping says which is which.
pub fn field_entries(version: &schema::CollectionVersion) -> Vec<FieldEntry> {
    version
        .fields
        .iter()
        .filter(|field| !field.id.is_empty())
        .zip(1u32..)
        .map(|(field, short_id)| FieldEntry {
            name: field.name.clone(),
            field_id: field.id.clone(),
            short_id,
        })
        .collect()
}
