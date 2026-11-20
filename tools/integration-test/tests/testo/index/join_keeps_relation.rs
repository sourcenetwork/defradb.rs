//! A join that starts from an indexed child keeps the relations joined before it.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.many_to_many.join-collection")]
fn a_join_from_an_indexed_child_keeps_an_earlier_join() {
    let mut actions = vec![AddCollection {
        sdl: r#"
            type Student { name: String @index }
            type Course { name: String @index }
            type Enrollment {
                student: Student @relation(name: "student_enrollments") @index
                course: Course @relation(name: "course_enrollments") @index
            }
        "#
        .into(),
        ..Default::default()
    }
    .into()];
    for (collection, name, id) in [
        ("Student", "Alice", "alice"),
        ("Student", "Bob", "bob"),
        ("Course", "Math", "math"),
        ("Course", "Science", "science"),
    ] {
        actions.push(
            AddDoc {
                collection: collection.into(),
                doc_map: Some(json!({ "name": name })),
                common: Common {
                    capture: captures([(id, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    for (student, course) in [("alice", "math"), ("alice", "science"), ("bob", "math")] {
        actions.push(
            AddDoc {
                collection: "Enrollment".into(),
                doc_map: Some(json!({ "student": m::r(student), "course": m::r(course) })),
                ..Default::default()
            }
            .into(),
        );
    }
    for request in [
        "query { Enrollment(filter: {course: {name: {_eq: \"Math\"}}}, \
         order: {student: {name: ASC}}) { student { name } } }",
        "query { Enrollment(filter: {course: {name: {_eq: \"Math\"}}}, \
         order: {student: {name: ASC}}) { student { name } course { name } } }",
    ] {
        let with_course = request.contains("course { name }");
        let row = |student: &str| {
            if with_course {
                json!({ "student": { "name": student }, "course": { "name": "Math" } })
            } else {
                json!({ "student": { "name": student } })
            }
        };
        actions.push(
            Request {
                request: request.into(),
                expected_data: json!({ "Enrollment": [row("Alice"), row("Bob")] }),
                ..Default::default()
            }
            .into(),
        );
    }
    TestCase::from_actions(actions).execute();
}
