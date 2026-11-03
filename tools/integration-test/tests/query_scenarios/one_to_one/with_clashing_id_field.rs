//! Generated from the conformance scenarios of tests/integration/query/one_to_one/with_clashing_id_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.relation-id")]
fn query_one_to_one_with_clashing_id_field_on_primary() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Book {
                        name: String
                        _authorID: Int
                        author: Author @primary
                    }

                    type Author {
                        name: String
                        published: Book
                    }
                "#
            .into(),
            expected_error: Some("duplicate field. Name: _authorID".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.relation-id")]
fn query_one_to_one_with_clashing_id_field_on_secondary() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Book {
                        name: String
                        _authorID: Int
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#
            .into(),
            expected_error: Some("duplicate field. Name: _authorID".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
