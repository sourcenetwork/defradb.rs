//! Which field type declarations adding a collection accepts and which it
//! refuses (#3267). A refusal has Go's text under every client, and a Rust
//! node also gives it the code `UNSUPPORTED_FIELD_TYPE`.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;
use integration_test::ClientType;

const CODE: &str = "UNSUPPORTED_FIELD_TYPE";

/// The clients a refusal's code reaches a test through.
const CODE_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp];

/// The clients a refusal is checked by its text alone: a Go node sends no
/// code, and the CLI prints only the message.
const TEXT_CLIENTS: &[ClientType] = &[ClientType::GoHttp, ClientType::Cli];

fn add(sdl: String, error: Option<&str>, code: Option<&str>) -> AddCollection {
    AddCollection {
        sdl: sdl.into(),
        expected_error: error.map(str::to_owned),
        expected_error_code: code.map(str::to_owned),
        ..Default::default()
    }
}

/// Adding `User` with the field `value: <declared>`, beside an `Author`
/// collection a relation may name.
fn with_field(declared: &str) -> String {
    format!("type Author {{ name: String }} type User {{ value: {declared} }}")
}

fn accepted(sdl: String) {
    TestCase::from_actions(vec![add(sdl, None, None).into()]).execute();
}

fn refused(declared: &str, error: &str) {
    TestCase {
        client_types: Some(TEXT_CLIENTS),
        ..TestCase::from_actions(vec![add(with_field(declared), Some(error), None).into()])
    }
    .execute();
    TestCase {
        client_types: Some(CODE_CLIENTS),
        ..TestCase::from_actions(vec![
            add(with_field(declared), Some(error), Some(CODE)).into()
        ])
    }
    .execute();
}

/// One `User` field per declared type.
fn fields(declared: &[&str]) -> String {
    let fields: Vec<String> = declared
        .iter()
        .enumerate()
        .map(|(i, declared)| format!("f{i}: {declared}"))
        .collect();
    format!(
        "type Author {{ name: String }} type User {{ {} }}",
        fields.join(" ")
    )
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
fn every_scalar_and_a_relation_is_accepted_nillable() {
    accepted(fields(&[
        "String", "Int", "Float", "Float64", "Float32", "Boolean", "ID", "DateTime", "JSON",
        "Blob", "Author",
    ]));
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
fn every_scalar_but_id_is_accepted_non_null() {
    accepted(fields(&[
        "String!",
        "Int!",
        "Float!",
        "Float64!",
        "Float32!",
        "Boolean!",
        "DateTime!",
        "JSON!",
        "Blob!",
    ]));
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
fn lists_of_array_scalars_and_relations_are_accepted() {
    accepted(fields(&[
        "[String]",
        "[Int]",
        "[Float]",
        "[Float64]",
        "[Float32]",
        "[Boolean]",
        "[DateTime]",
    ]));
    accepted(
        "type Author { name: String books: [Book] } type Book { title: String author: Author }"
            .to_owned(),
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
fn lists_of_non_null_array_scalars_are_accepted() {
    accepted(fields(&[
        "[String!]",
        "[Int!]",
        "[Float!]",
        "[Float64!]",
        "[Float32!]",
        "[Boolean!]",
        "[DateTime!]",
    ]));
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_non_null_id_is_refused() {
    refused("ID!", "NonNull fields are not currently supported");
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_non_null_relation_is_refused() {
    refused("Author!", "NonNull fields are not currently supported");
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_non_null_scalar_list_is_refused() {
    refused("[Int]!", "NonNull fields are not currently supported");
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_non_null_list_of_non_null_scalars_is_refused() {
    refused("[Int!]!", "NonNull fields are not currently supported");
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_non_null_relation_list_is_refused() {
    refused("[Author]!", "NonNull fields are not currently supported");
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_non_null_ids_is_refused() {
    refused(
        "[ID!]",
        "NonNull variants for type are not supported. Type: ID",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_non_null_blobs_is_refused() {
    refused(
        "[Blob!]",
        "NonNull variants for type are not supported. Type: Blob",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_non_null_json_is_refused() {
    refused(
        "[JSON!]",
        "NonNull variants for type are not supported. Type: JSON",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_non_null_relations_is_refused() {
    refused(
        "[Author!]",
        "NonNull variants for type are not supported. Type: Author",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_ids_is_refused() {
    refused(
        "[ID]",
        "no type found for given name. Field: value, Kind: [ID]",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_blobs_is_refused() {
    refused(
        "[Blob]",
        "no type found for given name. Field: value, Kind: [Blob]",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_list_of_json_is_refused() {
    refused(
        "[JSON]",
        "no type found for given name. Field: value, Kind: [JSON]",
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.field_type_forms")]
#[mantra_macros::req_test("defra.collection_version.define.field_type_code")]
fn a_nested_list_is_refused() {
    refused(
        "[[Int]]",
        "nested list types are not supported. Object: User, Field: value",
    );
}
