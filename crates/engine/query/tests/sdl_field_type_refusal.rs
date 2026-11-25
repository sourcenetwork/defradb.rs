//! Every field type form the SDL accepts or refuses, and the typed refusal.

use query::sdl_parse::{parse_sdl, FieldTypeRefusal, FieldTypeRefusals};
use query::QueryError;

fn declared(ty: &str) -> Result<(), QueryError> {
    parse_sdl(&format!(
        "type Author {{ name: String books: [Book] }} type Book {{ author: Author }} type User {{ value: {ty} }}"
    ))
    .map(drop)
}

fn refusal(ty: &str) -> FieldTypeRefusal {
    match declared(ty) {
        Err(QueryError::UnsupportedFieldType(FieldTypeRefusals(mut refusals))) => {
            assert_eq!(refusals.len(), 1, "{ty}");
            refusals.remove(0)
        }
        other => panic!("{ty}: expected a typed refusal, got {other:?}"),
    }
}

const ARRAY_SCALARS: &[&str] = &[
    "String", "Int", "Float", "Float64", "Float32", "Boolean", "DateTime",
];

#[test]
fn accepted_forms_parse() {
    for ty in ARRAY_SCALARS {
        for form in ["{}", "{}!", "[{}]", "[{}!]"] {
            let declared_type = form.replace("{}", ty);
            assert!(declared(&declared_type).is_ok(), "{declared_type}");
        }
    }
    for ty in ["JSON", "Blob", "JSON!", "Blob!", "ID", "Author", "[Book]"] {
        assert!(declared(ty).is_ok(), "{ty}");
    }
}

#[test]
fn non_null_forms_are_refused() {
    for ty in [
        "ID!",
        "Author!",
        "[Int]!",
        "[Int!]!",
        "[Author]!",
        "[ID]!",
        "[[Int]]!",
    ] {
        assert_eq!(refusal(ty), FieldTypeRefusal::NonNull, "{ty}");
        assert_eq!(
            refusal(ty).to_string(),
            "NonNull fields are not currently supported"
        );
    }
}

#[test]
fn non_null_elements_without_an_array_kind_are_refused() {
    for ty in ["ID", "Blob", "JSON", "Author"] {
        let refused = refusal(&format!("[{ty}!]"));
        assert_eq!(
            refused.to_string(),
            format!("NonNull variants for type are not supported. Type: {ty}")
        );
    }
}

#[test]
fn lists_of_scalars_without_an_array_kind_are_refused() {
    for ty in ["ID", "Blob", "JSON"] {
        let refused = refusal(&format!("[{ty}]"));
        assert_eq!(
            refused.to_string(),
            format!("no type found for given name. Field: value, Kind: [{ty}]")
        );
    }
}

#[test]
fn nested_lists_are_refused() {
    for ty in ["[[Int]]", "[[Int]!]", "[[Int!]]"] {
        assert_eq!(
            refusal(ty).to_string(),
            "nested list types are not supported. Object: User, Field: value",
            "{ty}"
        );
    }
}

#[test]
fn a_refusal_carries_its_code() {
    let QueryError::UnsupportedFieldType(refusals) = declared("ID!").unwrap_err() else {
        panic!("expected a typed refusal");
    };
    assert_eq!(
        refusals.code(),
        query::sdl_parse::UNSUPPORTED_FIELD_TYPE_ERROR_CODE
    );
    assert_eq!(refusals.code(), "UNSUPPORTED_FIELD_TYPE");
}

#[test]
fn an_unknown_list_type_names_its_list_kind() {
    let error = declared("[NotAType]").unwrap_err();
    assert!(matches!(error, QueryError::Parse(_)), "{error:?}");
    assert!(error
        .to_string()
        .contains("no type found for given name. Field: value, Kind: [NotAType]"));
}
