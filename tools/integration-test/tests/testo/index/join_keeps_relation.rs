//! A join that starts from an indexed child keeps the relations joined before it.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

/// Students Alice and Bob, courses Math and Science, and three enrollments.
fn enrollments() -> Vec<Actions> {
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
        actions.push(add_doc(collection, json!({ "name": name }), Some(id)));
    }
    for (student, course) in [("alice", "math"), ("alice", "science"), ("bob", "math")] {
        actions.push(add_doc(
            "Enrollment",
            json!({ "student": m::r(student), "course": m::r(course) }),
            None,
        ));
    }
    actions
}

#[test]
#[mantra_macros::req_test("go.query.many_to_many.join-collection")]
fn a_join_from_an_indexed_child_keeps_an_earlier_join() {
    let mut actions = enrollments();
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

fn request(request: &str, expected_data: Value) -> Actions {
    Request {
        request: request.into(),
        expected_data,
        ..Default::default()
    }
    .into()
}

/// Books by two authors, with publishers, an owner and a review.
fn books() -> Vec<Actions> {
    let mut actions = vec![AddCollection {
        sdl: r#"
            type Owner { name: String publishers: [Publisher] }
            type Author { name: String @index books: [Book] }
            type Publisher { name: String books: [Book] owner: Owner @primary }
            type Book {
                title: String
                author: Author @index
                publisher: Publisher
                reviews: [Review]
            }
            type Review { text: String book: Book }
        "#
        .into(),
        ..Default::default()
    }
    .into()];
    actions.push(add_doc("Owner", json!({ "name": "Olga" }), Some("olga")));
    actions.push(add_doc("Author", json!({ "name": "Alice" }), Some("alice")));
    actions.push(add_doc("Author", json!({ "name": "Bob" }), Some("bob")));
    actions.push(add_doc(
        "Publisher",
        json!({ "name": "Penguin", "owner": m::r("olga") }),
        Some("penguin"),
    ));
    actions.push(add_doc(
        "Publisher",
        json!({ "name": "Other" }),
        Some("other"),
    ));
    for (title, author, publisher, id) in [
        ("A1", "alice", "penguin", Some("a1")),
        ("A2", "alice", "other", None),
        ("B1", "bob", "penguin", None),
    ] {
        actions.push(add_doc(
            "Book",
            json!({ "title": title, "author": m::r(author), "publisher": m::r(publisher) }),
            id,
        ));
    }
    actions.push(add_doc(
        "Review",
        json!({ "text": "great", "book": m::r("a1") }),
        None,
    ));
    actions
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn an_indexed_author_filter_keeps_a_nested_publisher_owner() {
    let mut actions = books();
    actions.push(request(
        r#"query { Book(filter: {author: {name: {_eq: "Alice"}}}, order: {title: ASC}) {
            title publisher { name owner { name } }
        } }"#,
        json!({ "Book": [
            { "title": "A1", "publisher": { "name": "Penguin", "owner": { "name": "Olga" } } },
            { "title": "A2", "publisher": { "name": "Other", "owner": null } },
        ] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn an_indexed_author_filter_keeps_an_earlier_one_to_many_join() {
    let mut actions = books();
    actions.push(request(
        r#"query { Book(filter: {author: {name: {_eq: "Alice"}}}, order: {title: ASC}) {
            title reviews { text } author { name }
        } }"#,
        json!({ "Book": [
            { "title": "A1", "reviews": [{ "text": "great" }], "author": { "name": "Alice" } },
            { "title": "A2", "reviews": [], "author": { "name": "Alice" } },
        ] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn an_indexed_author_filter_and_a_publisher_filter_both_apply() {
    let mut actions = books();
    actions.push(request(
        r#"query { Book(filter: {author: {name: {_eq: "Alice"}}, publisher: {name: {_eq: "Penguin"}}}) {
            title publisher { name }
        } }"#,
        json!({ "Book": [{ "title": "A1", "publisher": { "name": "Penguin" } }] }),
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn an_enrollment_filtered_on_course_and_student_keeps_both() {
    let mut actions = enrollments();
    actions.push(request(
        r#"query { Enrollment(filter: {course: {name: {_eq: "Math"}}, student: {name: {_eq: "Bob"}}}) {
            student { name } course { name }
        } }"#,
        json!({ "Enrollment": [{ "student": { "name": "Bob" }, "course": { "name": "Math" } }] }),
    ));
    TestCase::from_actions(actions).execute();
}
