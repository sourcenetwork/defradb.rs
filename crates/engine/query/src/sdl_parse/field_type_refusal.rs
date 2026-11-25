//! Field type declarations an SDL may not make, refused in Go's words.
//!
//! Go's `astTypeToKind` decides which shapes a field type may take: a
//! non-null field only of a scalar with a non-nillable kind, a list only of
//! scalars, non-null list elements only for scalars with an array kind, and
//! no nesting. Every refusal carries [`UNSUPPORTED_FIELD_TYPE_ERROR_CODE`].

use graphql_parser::schema::Type;

/// The code every [`FieldTypeRefusal`] reaches a client with.
pub const UNSUPPORTED_FIELD_TYPE_ERROR_CODE: &str = "UNSUPPORTED_FIELD_TYPE";

/// Scalars a field may declare non-null (`T!`).
const NON_NULL_SCALARS: &[&str] = &[
    "Boolean", "Int", "Float", "Float64", "Float32", "DateTime", "String", "Blob", "JSON",
];

/// Scalars a list may hold, with or without non-null elements.
const LIST_SCALARS: &[&str] = &[
    "Boolean", "Int", "Float", "Float64", "Float32", "DateTime", "String",
];

/// Scalars a list may not hold: Go reads `[ID]` as a list of a type named `ID`.
const UNLISTABLE_SCALARS: &[&str] = &["ID", "Blob", "JSON"];

/// A field type declaration that is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FieldTypeRefusal {
    /// A non-null field of a type with no non-nillable kind (`ID!`, a
    /// relation `Author!`), or a non-null list (`[Int]!`).
    #[error("NonNull fields are not currently supported")]
    NonNull,
    /// Non-null elements in a list of a type with no array kind of them
    /// (`[ID!]`, `[Blob!]`, `[JSON!]`, a relation `[Author!]`).
    #[error("NonNull variants for type are not supported. Type: {type_name}")]
    NonNullElements { type_name: String },
    /// A list of lists (`[[Int]]`).
    #[error("nested list types are not supported. Object: {object}, Field: {field}")]
    NestedList { object: String, field: String },
    /// A list of a scalar that has no array kind (`[ID]`, `[Blob]`, `[JSON]`).
    #[error("no type found for given name. Field: {field}, Kind: [{type_name}]")]
    UnlistedScalar { field: String, type_name: String },
    /// A field of a type no scalar or collection names (`NotAType`), refused
    /// beside one of the refusals above, so the SDL keeps their code. Alone it
    /// is a plain parse error.
    #[error("no type found for given name. Field: {field}, Kind: {kind}")]
    UnknownType { field: String, kind: String },
}

/// One or more refused field type declarations, one line each.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", lines(.0))]
pub struct FieldTypeRefusals(pub Vec<FieldTypeRefusal>);

impl FieldTypeRefusals {
    /// The code these refusals reach a client with.
    pub fn code(&self) -> &'static str {
        UNSUPPORTED_FIELD_TYPE_ERROR_CODE
    }
}

impl From<FieldTypeRefusal> for FieldTypeRefusals {
    fn from(refusal: FieldTypeRefusal) -> Self {
        Self(vec![refusal])
    }
}

fn lines(refusals: &[FieldTypeRefusal]) -> String {
    refusals
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The refusal of `field`'s declared type on `object`, checked in Go's order:
/// nesting, then non-null elements, then non-null.
pub(super) fn refusal(
    object: &str,
    field: &str,
    ty: &Type<'_, String>,
) -> Option<FieldTypeRefusal> {
    match ty {
        Type::NamedType(_) => None,
        Type::ListType(element) => match element.as_ref() {
            Type::ListType(_) => Some(nested(object, field)),
            Type::NonNullType(inner) => match inner.as_ref() {
                Type::ListType(_) => Some(nested(object, field)),
                Type::NamedType(name) if LIST_SCALARS.contains(&name.as_str()) => None,
                Type::NamedType(name) => Some(FieldTypeRefusal::NonNullElements {
                    type_name: name.clone(),
                }),
                Type::NonNullType(_) => Some(FieldTypeRefusal::NonNull),
            },
            Type::NamedType(_) => None,
        },
        Type::NonNullType(inner) => match inner.as_ref() {
            Type::NamedType(name) if NON_NULL_SCALARS.contains(&name.as_str()) => None,
            _ => Some(FieldTypeRefusal::NonNull),
        },
    }
}

/// The refusal of a list of `type_name`, which no array kind holds.
pub(super) fn unlisted(field: &str, type_name: &str) -> Option<FieldTypeRefusal> {
    UNLISTABLE_SCALARS
        .contains(&type_name)
        .then(|| FieldTypeRefusal::UnlistedScalar {
            field: field.to_owned(),
            type_name: type_name.to_owned(),
        })
}

fn nested(object: &str, field: &str) -> FieldTypeRefusal {
    FieldTypeRefusal::NestedList {
        object: object.to_owned(),
        field: field.to_owned(),
    }
}
