//! The chunk list is not a mutation input (`defra.chunk.protect.list_input`).

use integration_test::actions::IntrospectionRequest;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::add_article_collection;

#[test]
#[mantra_macros::req_test("defra.chunk.protect.list_input")]
fn the_chunk_list_is_not_a_mutation_input() {
    TestCase::from_actions(vec![
        add_article_collection(),
        IntrospectionRequest {
            request:
                r#"query { __type(name: "ArticleMutationInputArg") { inputFields { name } } }"#
                    .into(),
            expected_data: json!({ "__type": { "inputFields": [{ "name": "body" }, { "name": "title" }] } }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
