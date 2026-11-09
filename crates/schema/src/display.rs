//! The readable form `collection describe` prints a collection version in.

use serde_json::Value;

use crate::{CType, CollectionVersion, FieldKind, ScalarKind};

impl CollectionVersion {
    /// This version as JSON with each field's `Kind` and `Typ` rendered as
    /// readable strings (`"String"`, `"[Int!]"`, `"lww"`) instead of their
    /// numeric IDs. Go's `CollectionVersion.Display`. It deserializes back
    /// into the same version.
    pub fn display(&self) -> serde_json::Result<Value> {
        let mut value = serde_json::to_value(self)?;
        if let Some(fields) = value.get_mut("Fields").and_then(Value::as_array_mut) {
            for (field, description) in fields.iter_mut().zip(&self.fields) {
                if let Some(kind) = kind_name(&description.kind) {
                    field["Kind"] = Value::from(kind);
                }
                if let Some(typ) = ctype_name(&description.crdt_type) {
                    field["Typ"] = Value::from(typ);
                }
            }
        }
        Ok(value)
    }
}

/// A scalar or scalar-array kind's name; `None` keeps its number and
/// relation kinds keep their object form, as in Go.
fn kind_name(kind: &FieldKind) -> Option<&'static str> {
    match kind {
        FieldKind::Scalar(ScalarKind::None) => None,
        FieldKind::Scalar(_) | FieldKind::ScalarArray(_) => Some(kind.graphql_type_name()),
        FieldKind::Relation { .. } | FieldKind::SelfRef { .. } | FieldKind::Named { .. } => None,
    }
}

/// Go's `CType.String`, plus the Rust-only text and JSON CRDTs; custom and
/// unknown types keep their wire form so they round-trip.
fn ctype_name(ctype: &CType) -> Option<&'static str> {
    match ctype {
        CType::None => Some("none"),
        CType::LwwRegister => Some("lww"),
        CType::Object => Some("object"),
        CType::Composite => Some("composite"),
        CType::PnCounter => Some("pncounter"),
        CType::PCounter => Some("pcounter"),
        CType::Text => Some("text"),
        CType::Json => Some("json"),
        CType::Custom(_) | CType::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldDescription, ScalarArrayKind};

    fn field(name: &str, kind: FieldKind, crdt_type: CType) -> FieldDescription {
        FieldDescription::new("", name, kind, crdt_type)
    }

    fn version() -> CollectionVersion {
        let fields = vec![
            field("_docID", FieldKind::Scalar(ScalarKind::DocID), CType::None),
            field(
                "name",
                FieldKind::Scalar(ScalarKind::NillableString),
                CType::LwwRegister,
            ),
            field(
                "tags",
                FieldKind::ScalarArray(ScalarArrayKind::IntArray),
                CType::PnCounter,
            ),
            field(
                "owner",
                FieldKind::Relation {
                    collection_id: "bafyowner".into(),
                    is_array: false,
                },
                CType::None,
            ),
            field("raw", FieldKind::Scalar(ScalarKind::None), CType::None),
        ];
        CollectionVersion::new("Dog", "bafyversion", "bafycollection", fields)
    }

    #[test]
    fn renders_scalar_kinds_and_crdt_types_as_go_strings() {
        let display = version().display().unwrap();
        let fields = display["Fields"].as_array().unwrap();
        let rendered: Vec<_> = fields
            .iter()
            .map(|field| (field["Kind"].clone(), field["Typ"].clone()))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (Value::from("ID"), Value::from("none")),
                (Value::from("String"), Value::from("lww")),
                (Value::from("[Int!]"), Value::from("pncounter")),
                (
                    serde_json::json!({ "Array": false, "CollectionID": "bafyowner" }),
                    Value::from("none")
                ),
                (Value::from(0), Value::from("none")),
            ]
        );
    }

    #[test]
    fn round_trips_to_the_same_version() {
        let version = version();
        let back: CollectionVersion = serde_json::from_value(version.display().unwrap()).unwrap();
        assert_eq!(back, version);
    }
}
