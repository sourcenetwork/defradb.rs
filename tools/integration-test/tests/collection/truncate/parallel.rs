//! Generated from the conformance scenarios of tests/integration/collection/truncate/parallel_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, Parallel, PeerEventsStart, Request, Truncate, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.truncate.basic")]
fn truncate_collection_parallel_deletes_all_previously_existing_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common { retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1"),
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: vec![
                        AddDoc {
                            collection: "Users".into(),
                            doc_map: Some(json!({ "name": "Fred" })),
                            expected_data: json!(m::object(json!({ "error": "" }))),
                            common: Common { retry: true, envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                        AddDoc {
                            collection: "Users".into(),
                            doc_map: Some(json!({ "name": "Shahzad" })),
                            expected_data: json!(m::object(json!({ "error": "" }))),
                            common: Common { retry: true, envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                        AddDoc {
                            collection: "Users".into(),
                            doc_map: Some(json!({ "name": "Chris" })),
                            expected_data: json!(m::object(json!({ "error": "" }))),
                            common: Common { retry: true, envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                        Request {
                            request: r#"
                                query {
                                                            Users(filter: {name: {_eq: "Keenan"}}) {
                                                                name
                                                            }
                                                        }
                            "#.into(),
                            expected_data: json!({ "error": "", "result": { "data": { "Users": [] } } }),
                            common: Common { envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                        Truncate {
                            collection: "Users".into(),
                            expected_data: json!({ "error": "", "result": {} }),
                            common: Common { envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                ],
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {_or: [{name: {_eq: "John"}},{name: {_eq: "Islam"}}]}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
