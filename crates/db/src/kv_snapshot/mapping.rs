//! The field-mapping file: Go Shinzo's `kvFieldMappingFile`, which tells an
//! importing node which collection and field each short ID in a snapshot
//! names.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMapping {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "collectionShortID")]
    pub collection_short_id: u32,
    #[serde(default)]
    pub fields: Vec<FieldEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldEntry {
    pub name: String,
    #[serde(rename = "fieldID")]
    pub field_id: String,
    #[serde(rename = "shortID")]
    pub short_id: u32,
}
