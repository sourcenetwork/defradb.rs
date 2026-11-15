//! The generated collection has no mutations (`defra.chunk.protect.no_mutations`).

use integration_test::actions::Request;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::add_article_collection;

#[test]
#[mantra_macros::req_test("defra.chunk.protect.no_mutations")]
fn generated_collection_has_no_mutations() {
    TestCase::from_actions(vec![
        add_article_collection(),
        Request {
            request: "query { __schema { mutationType { fields { name } } } }".into(),
            expected_data: json!({ "__schema": { "mutationType": { "fields": [
                { "name": "add_Article" },
                { "name": "update_Article" },
                { "name": "upsert_Article" },
                { "name": "delete_Article" },
                { "name": "truncate_Article" },
            ] } } }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
