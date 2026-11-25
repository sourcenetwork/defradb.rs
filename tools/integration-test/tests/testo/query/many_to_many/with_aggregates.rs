//! Aggregates over the relation a join collection serves: each student's
//! enrollment grades, summed, averaged, bounded and counted, as in Go.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const JOIN_TYPES: &str = r#"
    type Course { name: String }
    type Enrollment {
        student: Student @relation(name: "student_enrollments")
        course: Course @relation(name: "course_enrollments")
        grade: Int
    }
"#;

/// Alice takes Math (90) and Science (70), Bob takes Math (80).
fn enrollments(student_type: &str) -> Vec<Actions> {
    let mut actions = vec![AddCollection {
        sdl: format!("{student_type}{JOIN_TYPES}").into(),
        ..Default::default()
    }
    .into()];
    for (collection, name, id) in [
        ("Student", "Alice", "alice"),
        ("Student", "Bob", "bob"),
        ("Course", "Math", "math"),
        ("Course", "Science", "science"),
    ] {
        actions.push(add_doc(collection, json!({ "name": name }), Some(id)));
    }
    for (student, course, grade) in [
        ("alice", "math", 90),
        ("alice", "science", 70),
        ("bob", "math", 80),
    ] {
        actions.push(add_doc(
            "Enrollment",
            json!({ "student": m::r(student), "course": m::r(course), "grade": grade }),
            None,
        ));
    }
    actions
}

fn add_doc(collection: &str, doc: Value, id: Option<&str>) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        common: Common {
            capture: id.map(|id| captures([(id, "/docID")])).unwrap_or_default(),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.query.many_to_many.aggregate")]
fn aggregates_over_join_collection_relation() {
    let mut actions = enrollments(
        r#"type Student {
            name: String
            enrollments: [Enrollment] @relation(name: "student_enrollments")
        }"#,
    );
    actions.push(
        Request {
            request: r#"query { Student(order: {name: ASC}) {
                name
                s: SUM(enrollments: {field: grade})
                a: AVG(enrollments: {field: grade})
                mi: MIN(enrollments: {field: grade})
                ma: MAX(enrollments: {field: grade})
                c: COUNT(enrollments: {})
            } }"#
                .into(),
            expected_data: json!({ "Student": [
                { "name": "Alice", "s": 160, "a": 80, "mi": 70, "ma": 90, "c": 2 },
                { "name": "Bob", "s": 80, "a": 80, "mi": 80, "ma": 80, "c": 1 },
            ] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.query.many_to_many.aggregate")]
fn filtered_aggregates_over_join_collection_relation() {
    let mut actions = enrollments(
        r#"type Student {
            name: String
            enrollments: [Enrollment] @relation(name: "student_enrollments")
        }"#,
    );
    actions.push(
        Request {
            request: r#"query { Student(order: {name: ASC}) {
                name
                sf: SUM(enrollments: {field: grade, filter: {grade: {_gt: 75}}})
                af: AVG(enrollments: {field: grade, filter: {course: {name: {_eq: "Math"}}}})
                mif: MIN(enrollments: {field: grade, filter: {grade: {_gt: 75}}})
                maf: MAX(enrollments: {field: grade, filter: {grade: {_lt: 85}}})
                cf: COUNT(enrollments: {filter: {grade: {_gt: 75}}})
            } }"#
                .into(),
            expected_data: json!({ "Student": [
                { "name": "Alice", "sf": 90, "af": 90, "mif": 90, "maf": 70, "cf": 1 },
                { "name": "Bob", "sf": 80, "af": 80, "mif": 80, "maf": 80, "cf": 1 },
            ] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.query.many_to_many.aggregate")]
fn aggregate_over_undeclared_relation_name_is_unknown_argument() {
    let mut actions = enrollments("type Student { name: String }");
    actions.push(
        Request {
            request: "query { Student { name SUM(student_enrollments: {field: grade}) } }".into(),
            expected_error: Some(
                "Unknown argument \"student_enrollments\" on field \"SUM\" of type \"Student\"."
                    .into(),
            ),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
