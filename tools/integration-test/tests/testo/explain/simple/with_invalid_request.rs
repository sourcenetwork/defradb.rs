//! An explained request that would fail when executed fails with the same error.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;

fn explain_fails(id: &'static str, request: &str, error: &str) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String!
                        genre: String
                        plot: String
                        publisher: Publisher
                    }

                    type Publisher {
                        name: String
                        books: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: request.into(),
                expected_error: Some(error.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn simple_explain_with_unknown_filter_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: simple) {
            Book(filter: {author: {name: {_like: "%vid%"}}}) { title }
        }"#,
        "Argument \"filter\" has invalid value {author: {name: {_like: \"%vid%\"}}}.\nIn field \"author\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn default_explain_with_unknown_filter_operator_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain {
            Book(filter: {genre: {_eqq: "Sci-fi"}}) { title }
        }"#,
        "Argument \"filter\" has invalid value {genre: {_eqq: \"Sci-fi\"}}.\nIn field \"genre\": In field \"_eqq\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn debug_explain_with_unknown_filter_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: debug) {
            Book(filter: {author: {_eq: "x"}}) { title }
        }"#,
        "Argument \"filter\" has invalid value {author: {_eq: \"x\"}}.\nIn field \"author\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn simple_explain_with_unknown_selected_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: simple) {
            Book { author }
        }"#,
        "Cannot query field \"author\" on type \"Book\".",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn default_explain_with_unknown_selected_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain { Book { title author } }"#,
        "Cannot query field \"author\" on type \"Book\".",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn debug_explain_with_unknown_selected_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: debug) { Book { author } }"#,
        "Cannot query field \"author\" on type \"Book\".",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn execute_explain_with_unknown_selected_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: execute) { Book { author } }"#,
        "Cannot query field \"author\" on type \"Book\".",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn simple_explain_with_unknown_selected_field_beside_relation_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: simple) { Book { author publisher { name } } }"#,
        "Cannot query field \"author\" on type \"Book\".",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn simple_explain_with_sub_selection_on_scalar_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: simple) { Book { title { name } } }"#,
        "Field \"title\" of type \"String!\" must not have a sub selection.",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn execute_explain_with_sub_selection_on_scalar_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"query @explain(type: execute) { Book { title { name } } }"#,
        "Field \"title\" of type \"String!\" must not have a sub selection.",
    );
}

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn simple_explain_update_with_unknown_filter_field_returns_error() {
    explain_fails(
        integration_test::scenario_id!(),
        r#"mutation @explain(type: simple) {
            update_Book(filter: {author: {_eq: "x"}}, input: {genre: "Drama"}) { title }
        }"#,
        "Argument \"filter\" has invalid value {author: {_eq: \"x\"}}.\nIn field \"author\": Unknown field.",
    );
}
