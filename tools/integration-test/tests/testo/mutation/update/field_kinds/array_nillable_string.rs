//! Generated from the conformance scenarios of tests/integration/mutation/update/field_kinds/array_nillable_string_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.update.nillable_arrays")]
fn mutation_update_with_array_of_strings_ints() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        preferredStrings: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "John",
                    "preferredStrings": ["", "the previous", "the first", "empty string", null],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({
                    "preferredStrings": ["", "the previous", null, "empty string", "blank string", "hitchi"],
                }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            preferredStrings
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "preferredStrings": [
                                "",
                                "the previous",
                                null,
                                "empty string",
                                "blank string",
                                "hitchi",
                            ],
                        },
                    ],
                }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
