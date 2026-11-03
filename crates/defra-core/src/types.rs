//! Core type definitions for DefraDB

use crate::doc_id::validate_doc_id_str;
use crate::{Error, Result};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Document identifier.
///
/// Canonical format: `{base32(version)}-{uuid}`
/// Example: `bae-c94acbfa-dd53-40d0-97f3-29ce16c333fc`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocId(String);

impl DocId {
    /// Create a new DocId from a string, validating the canonical format.
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        validate_doc_id_str(&id).map_err(|err| Error::InvalidDocumentId(err.to_string()))?;
        Ok(Self(id))
    }

    /// Create a DocId without validating the string format.
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for DocId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl Serialize for DocId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DocId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docid_creation() {
        let id = DocId::new_unchecked("bae-test123");
        assert_eq!(id.as_str(), "bae-test123");
    }

    #[test]
    fn test_docid_validation() {
        assert!(DocId::new("bae-c94acbfa-dd53-40d0-97f3-29ce16c333fc").is_ok());
        assert!(DocId::new("doc1").is_err());
    }

    #[test]
    fn test_docid_deserialization_rejects_invalid_values() {
        let err = serde_json::from_str::<DocId>("\"doc1\"").unwrap_err();
        assert!(err.to_string().contains("invalid document ID"));
    }
}
