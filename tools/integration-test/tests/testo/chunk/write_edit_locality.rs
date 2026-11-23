//! A typo fix in one paragraph of a long text writes one new chunk and leaves the others (`defra.chunk.write.locality`).

use integration_test::actions::{Actions, AddCollection, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article, CHUNKS};

const SCHEMA: &str = r#"type Article {
    title: String
    body: String @chunk(maxChars: 160)
}"#;

fn book(typo: bool) -> String {
    (0..40)
        .map(|i| {
            let verb = if typo && i == 20 { "takls" } else { "talks" };
            format!("Paragraph number {i} {verb} about its own small topic.")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The chunk holding paragraph 20, and every other chunk.
fn read_chunks(common: Common, expected: serde_json::Value) -> Actions {
    Request {
        request: format!(
            r#"query {{
                target: {CHUNKS}(filter: {{text: {{_like: "%number 20 %"}}}}) {{ _docID }}
                others: {CHUNKS}(filter: {{_not: {{text: {{_like: "%number 20 %"}}}}}}) {{ _docID }}
            }}"#
        )
        .into(),
        expected_data: expected,
        common,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.locality")]
fn editing_one_paragraph_changes_one_chunk() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: SCHEMA.into(),
            ..Default::default()
        }
        .into(),
        add_article(&book(false), "article"),
        read_chunks(
            Common {
                capture: captures([("others_before", "/data/others")]),
                ..Default::default()
            },
            json!({ "target": [{ "_docID": m::unique("ids") }], "others": m::nonempty() }),
        ),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": book(true) }),
            ..Default::default()
        }
        .into(),
        read_chunks(
            Common {
                unordered: vec!["/data/others".into()],
                ..Default::default()
            },
            json!({ "target": [{ "_docID": m::unique("ids") }], "others": m::r("others_before") }),
        ),
    ])
    .execute();
}
