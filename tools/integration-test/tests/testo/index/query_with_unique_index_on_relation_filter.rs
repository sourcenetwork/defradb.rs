//! Generated from the conformance scenarios of tests/integration/index/query_with_unique_index_on_relation_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_unique_composite_index_with_filter_on_indexed_relation_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        devices: [Device]
                    }

                    type Device  {
                        manufacturer: String 
                        owner: User @index(unique: true, includes: [{field: "manufacturer"}])
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_0"), "manufacturer": "Apple" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
					User {
						name
						devices(filter: {_ownerID: {_eq: ""#), Text::reference("doc0_0"), Text::from(r#""}}) {
							manufacturer
						}
					}
				}"#)]),
                expected_data: json!({ "User": [{ "devices": [{ "manufacturer": "Apple" }], "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
