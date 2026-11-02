//! Generated from the conformance scenarios of tests/integration/collection/truncate/filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::Filter;
use integration_test::actions::{AddCollection, AddDoc, Request, Truncate};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.truncate.filter")]
fn truncate_collection_with_filter_is_idempotent() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Jane" })),
                common: Common {
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                filter: Some(Filter {
                    kind: "map".into(),
                    value: Some(json!({ "_docID": { "_in": [m::r("doc0")] } })),
                }),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                filter: Some(Filter {
                    kind: "map".into(),
                    value: Some(json!({ "_docID": { "_in": [m::r("doc0")] } })),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "Jane" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.filter")]
fn truncate_collection_with_large_integer_filter_removes_only_exact_match() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 9007199254740992_i64 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 9007199254740993_i64 })),
                common: Common {
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                filter: Some(Filter {
                    kind: "map".into(),
                    value: Some(json!({ "age": { "_eq": 9007199254740993_i64 } })),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
