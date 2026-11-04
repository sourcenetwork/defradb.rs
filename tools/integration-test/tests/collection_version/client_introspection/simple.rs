//! Generated from the conformance scenarios of tests/integration/collection_version/client_introspection/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::Request;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.builtin_types")]
fn client_introspection_basic() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![Request {
            request: r#"
                    query IntrospectionQuery {
                      __schema {
                        queryType { name }
                        mutationType { name }
                        subscriptionType { name }
                        types {
                          ...FullType
                        }
                        directives {
                          name
                          description
                          locations
                          args {
                            ...InputValue
                          }
                        }
                      }
                    }

                    fragment FullType on __Type {
                      kind
                      name
                      description
                      fields(includeDeprecated: true) {
                        name
                        description
                        args {
                          ...InputValue
                        }
                        type {
                          ...TypeRef
                        }
                        isDeprecated
                        deprecationReason
                      }
                      inputFields {
                        ...InputValue
                      }
                      interfaces {
                        ...TypeRef
                      }
                      enumValues(includeDeprecated: true) {
                        name
                        description
                        isDeprecated
                        deprecationReason
                      }
                      possibleTypes {
                        ...TypeRef
                      }
                    }

                    fragment InputValue on __InputValue {
                      name
                      description
                      type { ...TypeRef }
                      defaultValue
                    }

                    fragment TypeRef on __Type {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                              ofType {
                                kind
                                name
                                ofType {
                                  kind
                                  name
                                  ofType {
                                    kind
                                    name
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                "#
            .into(),
            expected_data: json!(m::object(
                json!({ "__schema": m::object(json!({ "types": m::introspection_types() })) })
            )),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
