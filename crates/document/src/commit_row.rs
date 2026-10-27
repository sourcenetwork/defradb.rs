//! A commit row returned by commit queries.
//!
//! A commit is a DAG block, not a document of any collection, so it has no
//! collection definition and carries no CRDT types.

use rapidhash::RapidHashMap;

use crate::encoding::json_to_normal_value;
use crate::field::special::DOC_ID;
use crate::{DocID, Error, NormalValue, Result};

/// One commit's fields keyed by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitRow {
    id: Option<DocID>,
    values: RapidHashMap<String, NormalValue>,
}

impl CommitRow {
    /// Build a row from a JSON map. A `_docID` entry becomes the row's ID.
    pub fn from_map(mut map: RapidHashMap<String, serde_json::Value>) -> Result<Self> {
        let mut row = Self::default();
        if let Some(id_value) = map.remove(DOC_ID) {
            let id = id_value.as_str().ok_or_else(|| Error::InvalidFieldValue {
                field: DOC_ID.to_string(),
                message: format!("_docID must be a string, got: {}", id_value),
            })?;
            row.id = Some(DocID::from_string(id)?);
        }
        for (key, value) in map {
            row.values.insert(key, json_to_normal_value(value)?);
        }
        Ok(row)
    }

    /// The ID of the document the commit belongs to, if any.
    pub fn id(&self) -> Option<&DocID> {
        self.id.as_ref()
    }

    /// Get a field value by name.
    pub fn get(&self, field: &str) -> Option<&NormalValue> {
        self.values.get(field)
    }

    /// Set a field value.
    pub fn set(&mut self, field: impl Into<String>, value: impl Into<NormalValue>) {
        self.values.insert(field.into(), value.into());
    }

    /// All field values.
    pub fn values(&self) -> &RapidHashMap<String, NormalValue> {
        &self.values
    }
}
