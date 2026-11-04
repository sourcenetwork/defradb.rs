//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_max_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.aggregate.max.basic")]
fn query_inline_nillable_float_array_with_doc_id_and_max_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        likedIndexes: [Boolean!]
                        indexLikesDislikes: [Boolean]
                        favouriteIntegers: [Int!]
                        testScores: [Int]
                        favouriteFloats: [Float!]
                        pageRatings: [Float]
                        favouriteFloat32s: [Float32!]
                        pageRatings32: [Float32]
                        favouriteFloat64s: [Float64!]
                        pageRatings64: [Float64]
                        preferredStrings: [String!]
                        pageHeaders: [String]
                        preferredDateTimes: [DateTime!]
                        loginTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "name": "Shahzad", "pageRatings": [3.1425, 1e-11, 10, null] }),
                ),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users(docID: ""#,
                    ),
                    Text::reference("doc0"),
                    Text::from(
                        r#"") {
						name
						MAX(pageRatings: {})
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "MAX": 10.0, "name": "Shahzad" }] }),
                common: Common {
                    compare: Some(true),
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
