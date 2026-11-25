//! A GraphQL create, update and delete keep the parent's chunks exact (`defra.chunk.write.graphql`).

use integration_test::actions::Request;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;

use super::support::{
    add_article_collection, body, chunk_texts_are, gql, no_chunks, update_article,
};

#[test]
#[mantra_macros::req_test("defra.chunk.write.graphql")]
fn graphql_create_update_and_delete_keep_chunks_exact() {
    TestCase::from_actions(vec![
        add_article_collection(),
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: {{title: "t", body: "{}"}}) {{ _docID }} }}"#,
                gql(&body(6, "rivers"))
            )
            .into(),
            common: Common {
                capture: captures([("article", "/data/add_Article/0/_docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&body(6, "rivers")], None),
        update_article("article", &body(4, "mountains")),
        chunk_texts_are(&[&body(4, "mountains")], None),
        Request {
            request: Text::concat([
                Text::from(r#"mutation { delete_Article(docID: ""#),
                Text::reference("article"),
                Text::from(r#"") { _docID } }"#),
            ]),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}
