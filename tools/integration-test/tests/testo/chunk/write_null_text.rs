//! A body set to null shows no chunks, and setting it back shows the same chunks again (`defra.chunk.write.null_text`).

use integration_test::actions::{AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article_collection, body, chunks_are_listed, no_chunks, read_chunks, update_article,
};

fn body_is(expected: serde_json::Value) -> integration_test::actions::Actions {
    Request {
        request: "query { Article { body } }".into(),
        expected_data: json!({ "Article": [{ "body": expected }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.null_text")]
fn a_null_body_hides_the_chunks_until_the_text_is_back() {
    let rivers = body(3, "rivers");
    TestCase::from_actions(vec![
        add_article_collection(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t" })),
            common: Common {
                capture: captures([("article", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        no_chunks(None),
        update_article("article", &rivers),
        read_chunks("river", 3, None),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": null }),
            ..Default::default()
        }
        .into(),
        body_is(json!(null)),
        no_chunks(None),
        update_article("article", &rivers),
        body_is(json!(rivers)),
        chunks_are_listed("river", 3, None),
    ])
    .execute();
}
