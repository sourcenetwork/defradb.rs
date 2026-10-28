//! Generated from the conformance scenarios of tests/integration/view/simple/with_embeddings_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.source_only_features")]
fn view_simple_with_embeddings_does_not_generate_embedding() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#.into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "ollama", model: "nomic-embed-text",  url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            name
                            name_v
                        }
                    }
                "#.into(),
                expected_data: json!({ "UserView": [{ "name": "Alice", "name_v": null }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
