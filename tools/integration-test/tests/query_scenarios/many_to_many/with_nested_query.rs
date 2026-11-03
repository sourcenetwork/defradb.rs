//! Generated from the conformance scenarios of tests/integration/query/many_to_many/with_nested_query_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.many_to_many.join-collection")]
fn many_to_many_query_from_secondary_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Student {
                        name: String
                        enrollments: [Enrollment] @relation(name: "student_enrollments")
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
                                        Student(filter: {name: {_eq: "Alice"}}) {
                                            name
                                            enrollments(order: {course: {name: ASC}}) {
                                                course { name }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Student": [
                        {
                            "enrollments": [
                                { "course": { "name": "Math" } },
                                { "course": { "name": "Science" } },
                            ],
                            "name": "Alice",
                        },
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
