//! Port of Go `tests/integration/collection_version/similarity_test.go`.

use integration_test::actions::{AddCollection, IntrospectionRequest};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const FIELD_DESCRIPTION: &str = "\nReturns how similar the given vector is to the specified field's value. The metric is\n whichever one the field's vector index was created with, defaulting to cosine similarity\n when the field has no vector index. Higher values are more similar for every metric.\n";

const VECTOR_DESCRIPTION: &str = "\nThe vector to compare the field's value against. It must have the same number of\n dimensions as the values stored in the field.\n";

fn argument(field: &str, scalar: &str) -> Value {
    json!({
        "name": field,
        "description": format!("Compares the given vector against the {field} field."),
        "type": {
            "name": format!("Users__{field}__SimilaritySelector"),
            "inputFields": [{
                "name": "vector",
                "description": VECTOR_DESCRIPTION,
                "type": {
                    "kind": "NON_NULL",
                    "name": null,
                    "ofType": {
                        "kind": "LIST",
                        "name": null,
                        "ofType": {
                            "kind": "NON_NULL",
                            "name": null,
                            "ofType": { "kind": "SCALAR", "name": scalar },
                        },
                    },
                },
            }],
        },
    })
}

fn case(sdl: &str, args: Vec<Value>) -> TestCase {
    TestCase {
        actions: vec![
            AddCollection {
                sdl: sdl.into(),
                ..Default::default()
            }
            .into(),
            IntrospectionRequest {
                request: r#"query {
                    __type (name: "Users") {
                        name
                        fields {
                            name
                            description
                            args {
                                name
                                description
                                type {
                                    name
                                    inputFields {
                                        name
                                        description
                                        type {
                                            name
                                            kind
                                            ofType {
                                                name
                                                kind
                                                ofType {
                                                    name
                                                    kind
                                                    ofType { name kind }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }"#
                .into(),
                expected_data: json!({ "__type": {
                    "name": "Users",
                    "fields": [{
                        "name": "SIMILARITY",
                        "description": FIELD_DESCRIPTION,
                        "args": args,
                    }],
                } }),
                contains_data: true,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
}

#[test]
fn capable_field_float32_array() {
    case(
        "type Users { someVector: [Float32!] }",
        vec![argument("someVector", "Float32")],
    )
    .execute();
}

#[test]
fn capable_field_int_array() {
    case(
        "type Users { someVector: [Int!] }",
        vec![argument("someVector", "Int")],
    )
    .execute();
}

#[test]
fn capable_fields_int_array_and_float32_array() {
    case(
        "type Users { someVectorFloat32: [Float32!]  someVectorInt: [Int!] }",
        vec![
            argument("someVectorFloat32", "Float32"),
            argument("someVectorInt", "Int"),
        ],
    )
    .execute();
}
