//! GraphQL array creates, filtered updates and deletes, and upserts keep every parent's chunks exact (`defra.chunk.write.graphql_batch`).

use integration_test::actions::{Actions, Request};
use integration_test::test_case::TestCase;

use super::support::{add_article_collection, body, chunk_texts_are, gql, no_chunks};

fn mutation(request: String) -> Actions {
    Request {
        request: request.into(),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.graphql_batch")]
fn graphql_batch_writes_keep_each_parents_chunks_exact() {
    let (rivers, lakes, hills) = (body(3, "rivers"), body(2, "lakes"), body(2, "hills"));
    let (caves, dunes) = (body(1, "caves"), body(2, "dunes"));
    TestCase::from_actions(vec![
        add_article_collection(),
        mutation(format!(
            r#"mutation {{ add_Article(input: [{{title: "a", body: "{}"}}, {{title: "b", body: "{}"}}]) {{ _docID }} }}"#,
            gql(&rivers),
            gql(&lakes)
        )),
        chunk_texts_are(&[&rivers, &lakes], None),
        mutation(format!(
            r#"mutation {{ update_Article(filter: {{}}, input: {{body: "{}"}}) {{ _docID }} }}"#,
            gql(&hills)
        )),
        chunk_texts_are(&[&hills, &hills], None),
        mutation(r#"mutation { delete_Article(filter: {title: {_eq: "a"}}) { _docID } }"#.into()),
        chunk_texts_are(&[&hills], None),
        mutation(format!(
            r#"mutation {{ upsert_Article(filter: {{title: {{_eq: "b"}}}}, add: {{title: "b"}}, update: {{body: "{}"}}) {{ _docID }} }}"#,
            gql(&caves)
        )),
        mutation(format!(
            r#"mutation {{ upsert_Article(filter: {{title: {{_eq: "c"}}}}, add: {{title: "c", body: "{}"}}, update: {{title: "c"}}) {{ _docID }} }}"#,
            gql(&dunes)
        )),
        chunk_texts_are(&[&caves, &dunes], None),
        mutation(r#"mutation { delete_Article(filter: {}) { _docID } }"#.into()),
        no_chunks(None),
    ])
    .execute();
}
