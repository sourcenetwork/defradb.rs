//! The document API keeps the parent's chunks exact (`defra.chunk.write.document_api`).

use integration_test::actions::{DeleteDoc, UpdateDoc};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article, add_article_collection, body, chunks_are_listed, no_chunks, read_list,
};

#[test]
#[mantra_macros::req_test("defra.chunk.write.document_api")]
fn document_api_writes_keep_chunks_exact() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(5, "deserts"), "article"),
        read_list("created", 5, None),
        chunks_are_listed("created", 5, None),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": body(3, "oceans") }),
            ..Default::default()
        }
        .into(),
        read_list("updated", 3, None),
        chunks_are_listed("updated", 3, None),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}
