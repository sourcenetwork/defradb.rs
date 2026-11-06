//! Generated from the conformance scenarios of tests/integration/collection_version/duplicate_relation_name_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.relations.relation_names")]
fn collection_version_duplicate_relation_name_with_relation_directive() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Book {
                        title: String
                        author: Person @relation(name: "book_author")
                        reviewer: Person @relation(name: "book_reviewer")
                    }

                    type Person {
                        name: String
                        authoredBooks: [Book] @relation(name: "book_author")
                        reviewedBooks: [Book] @relation(name: "book_reviewer")
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.relations.relation_names")]
fn collection_version_duplicate_relation_name_without_relation_directive() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        author: Person
                        reviewer: Person
                    }

                    type Person {
                        name: String
                        authoredBooks: [Book]
                        reviewedBooks: [Book]
                    }
                "#.into(),
                expected_error: Some("relation name is not unique within collection. Field: author, RelationName: book_person".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.relations.relation_names")]
fn collection_version_self_relation_primary_secondary_pair_with_same_relation_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Person {
                        name: String
                        friend: Person
                        friends: [Person]
                    }
                "#
            .into(),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
