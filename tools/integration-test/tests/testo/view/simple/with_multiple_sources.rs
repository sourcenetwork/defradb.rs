use integration_test::actions::{AddCollection, AddView, GetCollections};
use integration_test::test_case::TestCase;
use serde_json::json;

/// A view takes one source type, so a source query that selects two must be
/// refused rather than silently keep the first. Go still accepts it, so the
/// test is listed in .github/testo/divergences.txt (sourcenetwork/defradb.rs#2594).
#[test]
#[mantra_macros::req_test("defra.view.add_checks")]
fn add_view_with_two_source_selections_fails_and_creates_no_view() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                    type Book {
                        title: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                    Book {
                        title
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                expected_error: Some("invalid view query: multiple selections found".into()),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("UserView".into()),
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
