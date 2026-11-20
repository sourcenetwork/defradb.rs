//! Generated from the conformance scenarios of tests/integration/query/many_to_many/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.many_to_many.join-collection")]
fn many_to_many_query_from_join_collection_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Student {
                        name: String
                    }

                    type Course {
                        name: String
                    }

                    type Enrollment {
                        student: Student @relation(name: "student_enrollments")
                        course: Course @relation(name: "course_enrollments")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Student".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Student".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Course".into(),
                doc_map: Some(json!({ "name": "Math" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Course".into(),
                doc_map: Some(json!({ "name": "Science" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Enrollment".into(),
                doc_map: Some(json!({ "course": m::r("doc1_0"), "student": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Enrollment".into(),
                doc_map: Some(json!({ "course": m::r("doc1_1"), "student": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Enrollment".into(),
                doc_map: Some(json!({ "course": m::r("doc1_0"), "student": m::r("doc0_1") })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Enrollment(
                                            filter: {course: {name: {_eq: "Math"}}}
                                            order: {student: {name: ASC}}
                                        ) {
                                            student { name }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Enrollment": [
                        { "student": { "name": "Alice" } },
                        { "student": { "name": "Bob" } },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Enrollment(
                                            filter: {student: {name: {_eq: "Alice"}}}
                                            order: {course: {name: ASC}}
                                        ) {
                                            course { name }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Enrollment": [
                        { "course": { "name": "Math" } },
                        { "course": { "name": "Science" } },
                    ],
                }),
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
