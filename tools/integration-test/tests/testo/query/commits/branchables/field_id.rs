//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/field_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.query-args")]
fn query_commits_branchables_with_field_name_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(
                                                filter: {fieldName: {_eq: null}}
                                            ) {
                                                collectionVersionId
                                                docID
                                                fieldName
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "collectionVersionId": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                            "docID": null,
                            "fieldName": null,
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
