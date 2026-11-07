//! A node that signs its blocks keeps the parent's chunks exact (`defra.chunk.write.signed`).

use integration_test::actions::{NewNode, Request};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;

use super::support::{
    add_article, add_article_collection, body, chunks_are_listed, gql, read_list,
};

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_stores_the_list_of_the_first_update() {
    TestCase::from_actions(vec![
        NewNode {
            enable_signing: true,
            ..Default::default()
        }
        .into(),
        add_article_collection(),
        add_article(&body(6, "rivers"), "article"),
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
    ])
    .execute();
}
