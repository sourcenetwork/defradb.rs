//! Generated from the conformance scenarios of tests/integration/index/query_with_composite_inxed_on_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn query_with_composite_index_on_many_to_one_with_multiple_indexed_child_nodes_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        devices: [Device]
                    }

                    type Device @index(includes: [{field: "_ownerID"}, {field: "_manufacturerID"}]) {
                        model: String
                        owner: User 
                        manufacturer: Manufacturer 
                    }

                    type Manufacturer {
                        name: String
                        devices: [Device]
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
                collection: "Manufacturer".into(),
                doc_map: Some(json!({ "name": "Apple" })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Manufacturer".into(),
                doc_map: Some(json!({ "name": "Sony" })),
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": m::r("doc2_0"),
                    "model": "MacBook Pro",
                    "owner": m::r("doc0_0"),
                })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": m::r("doc2_1"),
                    "model": "PlayStation 5",
                    "owner": m::r("doc0_0"),
                })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            devices {
                                                model
                                                owner {
                                                    name
                                                }
                                                manufacturer {
                                                    name
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [
                                {
                                    "manufacturer": { "name": "Apple" },
                                    "model": "MacBook Pro",
                                    "owner": { "name": "John" },
                                },
                                {
                                    "manufacturer": { "name": "Sony" },
                                    "model": "PlayStation 5",
                                    "owner": { "name": "John" },
                                },
                            ],
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
