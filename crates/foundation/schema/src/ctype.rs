//! CRDT type definitions
//!
//! # JSON Serialization Format (Go-compatible)
//!
//! CType is serialized as its integer value to match Go DefraDB:
//! - 0 = None
//! - 1 = LwwRegister
//! - 2 = Object
//! - 3 = Composite
//! - 4 = PnCounter
//! - 5 = PCounter
//! - 6 = Text (Rust-only)
//! - 7 = Json (Rust-only)
//! - 8 = Custom (Rust-only): a CRDT type registered on the node, named by the
//!   field definition's `customCrdt` key and by `{"custom": "<name>"}` in JSON

use crate::{CrdtName, FieldKind, ScalarKind};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Which CRDT to use for a field
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CType {
    /// No CRDT (for relations and special fields)
    None,
    /// Last-Write-Wins Register
    LwwRegister,
    /// Object CRDT (for embedded objects)
    Object,
    /// Composite CRDT (document level)
    Composite,
    /// Positive-Negative Counter (increment/decrement)
    PnCounter,
    /// Positive Counter (increment only)
    PCounter,
    /// Collaborative text: a Fugue sequence of characters (Rust-only)
    Text,
    /// JSON with a register per path, merged path by path (Rust-only)
    Json,
    /// A CRDT type registered on the node by name (Rust-only). A node that
    /// does not have it keeps the definition and refuses the field's writes
    /// and merges.
    Custom(CrdtName),
    /// Unknown CRDT type (preserves raw value for validation errors)
    Unknown(u8),
}

impl Serialize for CType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let CType::Custom(name) = self {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("custom", name.as_str())?;
            return map.end();
        }
        serializer.serialize_u8(self.to_u8())
    }
}

impl<'de> Deserialize<'de> for CType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        match &value {
            // Integer format (Go's default)
            serde_json::Value::Number(n) => {
                let kind = n
                    .as_u64()
                    .ok_or_else(|| de::Error::custom("CType must be a positive integer"))?
                    as u8;
                Ok(match kind {
                    0 => CType::None,
                    1 => CType::LwwRegister,
                    2 => CType::Object,
                    3 => CType::Composite,
                    4 => CType::PnCounter,
                    5 => CType::PCounter,
                    6 => CType::Text,
                    7 => CType::Json,
                    _ => CType::Unknown(kind), // Preserve unknown for validation
                })
            }
            // String format (for human-readable configs)
            serde_json::Value::String(s) => match s.as_str() {
                "None" | "none" | "NONE_CRDT" => Ok(CType::None),
                "LwwRegister" | "lww" | "LWW_REGISTER" => Ok(CType::LwwRegister),
                "Object" | "object" | "OBJECT" => Ok(CType::Object),
                "Composite" | "composite" | "COMPOSITE" => Ok(CType::Composite),
                "PnCounter" | "pncounter" | "PN_COUNTER" => Ok(CType::PnCounter),
                "PCounter" | "pcounter" | "P_COUNTER" => Ok(CType::PCounter),
                "Text" | "text" | "TEXT" => Ok(CType::Text),
                "Json" | "json" | "JSON" => Ok(CType::Json),
                _ => Err(de::Error::custom("unknown crdt string representation")),
            },
            serde_json::Value::Object(map) => match (map.len(), map.get("custom")) {
                (1, Some(serde_json::Value::String(name))) => CrdtName::new(name)
                    .map(CType::Custom)
                    .map_err(de::Error::custom),
                _ => Err(de::Error::custom(
                    "a custom CRDT type is {\"custom\": \"<name>\"}",
                )),
            },
            // Null decodes as Go's zero value
            serde_json::Value::Null => Ok(CType::None),
            _ => Err(de::Error::custom("CType must be a number or string")),
        }
    }
}

impl CType {
    /// The code of every custom type; the definition names which one.
    pub const CUSTOM_CODE: u8 = 8;

    /// Convert to u8 representation for CID generation
    pub fn to_u8(self) -> u8 {
        match self {
            CType::None => 0,
            CType::LwwRegister => 1,
            CType::Object => 2,
            CType::Composite => 3,
            CType::PnCounter => 4,
            CType::PCounter => 5,
            CType::Text => 6,
            CType::Json => 7,
            CType::Custom(_) => Self::CUSTOM_CODE,
            CType::Unknown(v) => v,
        }
    }

    /// The custom type's name a field definition carries beside its code.
    pub fn custom_name(self) -> Option<CrdtName> {
        match self {
            CType::Custom(name) => Some(name),
            _ => None,
        }
    }

    /// The type a field definition's code and custom name declare: the inverse
    /// of [`to_u8`](Self::to_u8) with [`custom_name`](Self::custom_name). A
    /// custom code without a valid name, or a name beside another code, is
    /// unknown.
    pub fn from_definition(code: u8, custom: Option<&str>) -> Self {
        match (code, custom) {
            (Self::CUSTOM_CODE, Some(name)) => {
                CrdtName::new(name).map_or(CType::Unknown(code), CType::Custom)
            }
            (_, Some(_)) => CType::Unknown(code),
            (code, None) => Self::from_u8(code),
        }
    }

    /// Convert from u8 representation
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => CType::None,
            1 => CType::LwwRegister,
            2 => CType::Object,
            3 => CType::Composite,
            4 => CType::PnCounter,
            5 => CType::PCounter,
            6 => CType::Text,
            7 => CType::Json,
            v => CType::Unknown(v),
        }
    }

    /// Check if this CRDT type is compatible with a field kind
    pub fn is_compatible_with(&self, kind: &FieldKind) -> bool {
        match self {
            CType::None => true,
            CType::LwwRegister => true,
            CType::Object => kind.is_object(),
            CType::Composite => true,
            // Counters only work with numeric types
            CType::PnCounter | CType::PCounter => kind.is_numeric(),
            // Text is a sequence of characters
            CType::Text => kind
                .as_scalar()
                .is_some_and(|scalar| scalar.base_kind() == ScalarKind::NillableString),
            CType::Json => kind
                .as_scalar()
                .is_some_and(|scalar| scalar.base_kind() == ScalarKind::NillableJson),
            // A custom type takes any scalar value; its operations refuse
            // the ones it cannot hold.
            CType::Custom(_) => kind.is_scalar(),
            CType::Unknown(_) => false,
        }
    }

    /// Returns true if this is a counter type
    pub fn is_counter(&self) -> bool {
        matches!(self, CType::PnCounter | CType::PCounter)
    }

    /// Returns true if this is the Text type
    pub fn is_text(&self) -> bool {
        matches!(self, CType::Text)
    }

    /// Returns true if this is the Json type
    pub fn is_json(&self) -> bool {
        matches!(self, CType::Json)
    }

    /// Returns true if this counter type allows decrement (negative increments)
    ///
    /// - PnCounter (Positive-Negative Counter): allows both increment and decrement
    /// - PCounter (Positive Counter): only allows increment
    pub fn allows_decrement(&self) -> bool {
        matches!(self, CType::PnCounter)
    }
}

impl fmt::Display for CType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CType::None => write!(f, "None"),
            CType::LwwRegister => write!(f, "LwwRegister"),
            CType::Object => write!(f, "Object"),
            CType::Composite => write!(f, "Composite"),
            CType::PnCounter => write!(f, "PnCounter"),
            CType::PCounter => write!(f, "PCounter"),
            CType::Text => write!(f, "Text"),
            CType::Json => write!(f, "Json"),
            CType::Custom(name) => write!(f, "{name}"),
            CType::Unknown(v) => write!(f, "Unknown({})", v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_compatibility() {
        assert!(CType::PnCounter.is_compatible_with(&FieldKind::int()));
        assert!(CType::PnCounter.is_compatible_with(&FieldKind::float64()));
        assert!(CType::PnCounter.is_compatible_with(&FieldKind::float32()));
        assert!(!CType::PnCounter.is_compatible_with(&FieldKind::string()));
        assert!(!CType::PnCounter.is_compatible_with(&FieldKind::bool()));
        assert!(!CType::PCounter.is_compatible_with(&FieldKind::datetime()));
    }

    #[test]
    fn text_is_only_for_strings() {
        assert!(CType::Text.is_compatible_with(&FieldKind::string()));
        assert!(CType::Text.is_compatible_with(&FieldKind::Scalar(ScalarKind::String)));
        assert!(!CType::Text.is_compatible_with(&FieldKind::int()));
        assert!(!CType::Text.is_compatible_with(&FieldKind::bool()));
        assert!(CType::Text.is_text());
        assert!(!CType::Text.is_counter());
    }

    #[test]
    fn text_serializes_as_six_and_parses_from_its_names() {
        assert_eq!(serde_json::to_string(&CType::Text).unwrap(), "6");
        assert_eq!(serde_json::from_str::<CType>("6").unwrap(), CType::Text);
        assert_eq!(
            serde_json::from_str::<CType>(r#""text""#).unwrap(),
            CType::Text
        );
        assert_eq!(CType::from_u8(CType::Text.to_u8()), CType::Text);
        assert_eq!(CType::Text.to_string(), "Text");
    }

    #[test]
    fn json_is_crdt_seven_on_json_fields_only() {
        assert!(CType::Json.is_compatible_with(&FieldKind::json()));
        assert!(!CType::Json.is_compatible_with(&FieldKind::string()));
        assert!(CType::Json.is_json());
        assert_eq!(serde_json::to_string(&CType::Json).unwrap(), "7");
        assert_eq!(
            serde_json::from_str::<CType>("\"json\"").unwrap(),
            CType::Json
        );
        assert_eq!(CType::from_u8(7), CType::Json);
        assert_eq!(CType::Json.to_string(), "Json");
    }

    #[test]
    fn a_custom_type_carries_its_name_beside_code_eight() {
        let custom = CType::Custom(CrdtName::new("maxreg").unwrap());
        assert_eq!(custom.to_u8(), 8);
        assert_eq!(custom.custom_name().unwrap().as_str(), "maxreg");
        assert_eq!(CType::from_definition(8, Some("maxreg")), custom);
        assert_eq!(CType::from_definition(8, None), CType::Unknown(8));
        assert_eq!(CType::from_definition(8, Some("lww")), CType::Unknown(8));
        assert_eq!(CType::from_definition(1, Some("maxreg")), CType::Unknown(1));
        assert_eq!(custom.to_string(), "maxreg");
        assert!(custom.is_compatible_with(&FieldKind::int()));
        assert!(!custom.is_compatible_with(&FieldKind::relation("users", false)));

        let json = serde_json::to_string(&custom).unwrap();
        assert_eq!(json, r#"{"custom":"maxreg"}"#);
        assert_eq!(serde_json::from_str::<CType>(&json).unwrap(), custom);
        assert!(serde_json::from_str::<CType>(r#"{"custom":"Max"}"#).is_err());
        assert!(serde_json::from_str::<CType>(r#"{"other":"maxreg"}"#).is_err());
    }

    #[test]
    fn every_native_code_reads_back_without_a_name() {
        for code in 0..=8 {
            let ctype = CType::from_definition(code, None);
            assert_eq!(ctype, CType::from_u8(code));
            assert_eq!(ctype.to_u8(), code);
            assert_eq!(ctype.custom_name(), None);
        }
    }

    #[test]
    fn test_lww_compatibility() {
        assert!(CType::LwwRegister.is_compatible_with(&FieldKind::string()));
        assert!(CType::LwwRegister.is_compatible_with(&FieldKind::int()));
        assert!(CType::LwwRegister.is_compatible_with(&FieldKind::bool()));
    }

    #[test]
    fn test_object_compatibility() {
        assert!(CType::Object.is_compatible_with(&FieldKind::relation("users", false)));
        assert!(!CType::Object.is_compatible_with(&FieldKind::string()));
    }

    #[test]
    fn test_is_counter() {
        assert!(CType::PnCounter.is_counter());
        assert!(CType::PCounter.is_counter());
        assert!(!CType::LwwRegister.is_counter());
        assert!(!CType::None.is_counter());
    }

    #[test]
    fn test_display() {
        assert_eq!(CType::LwwRegister.to_string(), "LwwRegister");
        assert_eq!(CType::PnCounter.to_string(), "PnCounter");
    }

    #[test]
    fn test_serialization_values() {
        // Verify each variant serializes to the expected integer
        assert_eq!(serde_json::to_string(&CType::None).unwrap(), "0");
        assert_eq!(serde_json::to_string(&CType::LwwRegister).unwrap(), "1");
        assert_eq!(serde_json::to_string(&CType::Object).unwrap(), "2");
        assert_eq!(serde_json::to_string(&CType::Composite).unwrap(), "3");
        assert_eq!(serde_json::to_string(&CType::PnCounter).unwrap(), "4");
        assert_eq!(serde_json::to_string(&CType::PCounter).unwrap(), "5");
    }

    #[test]
    fn test_unknown_preserved() {
        let parsed: CType = serde_json::from_str("99").unwrap();
        assert_eq!(parsed, CType::Unknown(99));
        assert!(!matches!(
            parsed,
            CType::None
                | CType::LwwRegister
                | CType::Object
                | CType::Composite
                | CType::PnCounter
                | CType::PCounter
        ));
    }

    #[test]
    fn test_unknown_string_matches_go_error() {
        let err = serde_json::from_str::<CType>(r#""invalid""#).unwrap_err();
        assert!(err
            .to_string()
            .contains("unknown crdt string representation"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let ctypes = vec![
            CType::None,
            CType::LwwRegister,
            CType::Object,
            CType::Composite,
            CType::PnCounter,
            CType::PCounter,
        ];

        for ct in ctypes {
            let json = serde_json::to_string(&ct).unwrap();
            let parsed: CType = serde_json::from_str(&json).unwrap();
            assert_eq!(ct, parsed);
        }
    }
}
