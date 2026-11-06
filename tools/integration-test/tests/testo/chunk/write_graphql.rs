//! A GraphQL create, update and delete keep the parent's chunks exact (`defra.chunk.write.graphql`).

use integration_test::actions::Request;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;

use super::support::{add_article_collection, body, chunks_are_listed, gql, no_chunks, read_list};

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
        read_list("created", 6, None),
        chunks_are_listed("created", 6, None),
        Request {
            request: Text::concat([
                Text::from(r#"mutation { update_Article(docID: ""#),
                Text::reference("article"),
                Text::from(format!(
                    r#"", input: {{body: "{}"}}) {{ _docID }} }}"#,
                    gql(&body(4, "mountains"))
                )),
            ]),
            ..Default::default()
        }
        .into(),
        read_list("updated", 4, None),
        chunks_are_listed("updated", 4, None),
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
