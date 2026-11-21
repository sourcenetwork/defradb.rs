use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.offset_limit")]
fn ordered_index_offset_limit_returns_the_requested_page() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type T {
                        s: Int @index
                        u: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 0, "u": 0 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 1, "u": 1 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 2, "u": 2 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 3, "u": 3 })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 4, "u": 4 })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 5, "u": 5 })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 6, "u": 6 })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 7, "u": 7 })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 8, "u": 8 })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "T".into(),
                doc_map: Some(json!({ "s": 9, "u": 9 })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                        T(order: {s: ASC}, limit: 3, offset: 2) {
                            s
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 5 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        T(order: {s: ASC}, limit: 3, offset: 2) {
                            s
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "T": [
                        { "s": 2 },
                        { "s": 3 },
                        { "s": 4 },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        T(order: {u: ASC}, limit: 3, offset: 2) {
                            u
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "T": [
                        { "u": 2 },
                        { "u": 3 },
                        { "u": 4 },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
