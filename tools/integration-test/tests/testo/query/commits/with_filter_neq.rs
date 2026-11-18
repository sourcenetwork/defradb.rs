//! `_commits` filtered with `fieldName: {_neq: …}`, the inequality operator
//! the standard query filters and Go name `_neq`.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.commits.field_name_neq")]
fn commits_filter_field_name_neq_excludes_the_named_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_neq: "_C"}}) {
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [{ "fieldName": "age" }, { "fieldName": "name" }]
                }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
