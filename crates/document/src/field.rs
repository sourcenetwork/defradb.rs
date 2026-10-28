//! Field types for document structure

use schema::CType;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A field identifier within a document.
///
/// Fields have a name and a CRDT type that determines how conflicts are resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Field {
    /// The field name
    name: String,
    /// The CRDT type for this field
    crdt_type: CType,
}

impl Field {
    /// Create a new field with the given name and CRDT type.
    ///
    /// Returns an error if the name is empty.
    pub fn new(name: impl Into<String>, crdt_type: CType) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::EmptyFieldName);
        }
        Ok(Self { name, crdt_type })
    }

    /// Get the field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the CRDT type for this field.
    pub fn crdt_type(&self) -> CType {
        self.crdt_type
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Special field names used by DefraDB
pub mod special {
    /// The document ID field name
    pub const DOC_ID: &str = "_docID";
    /// The deleted marker field name
    pub const DELETED: &str = "_deleted";
    /// The version field name
    pub const VERSION: &str = "_version";
}
