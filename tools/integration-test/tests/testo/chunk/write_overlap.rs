//! With `overlap` above 0, a chunk starts with the last sentence of the chunk before it (`defra.chunk.write.overlap`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::on;

const SCHEMA: &str = r#"type Note {
    body: String @chunk(maxChars: 160, overlap: 1)
}"#;

const CHUNKS: &str = "Note__body_chunk";

const FIRST: &str = "The river starts high in the cold mountains and runs down past many quiet towns. It ends at sea.\n\n";
const SECOND: &str = "The road follows the river through the valley for most of its long winding way. It ends in town.\n\n";
const THIRD: &str = "The hill rises above the valley and the town and gives a wide view of everything. It is green.";

#[test]
#[mantra_macros::req_test("defra.chunk.write.overlap")]
fn a_chunk_repeats_the_last_sentence_of_the_one_before() {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    TestCase::from_actions(vec![
        AddCollection {
            sdl: SCHEMA.into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(json!({ "body": format!("{FIRST}{SECOND}{THIRD}") })),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ {CHUNKS} {{ text }} }}").into(),
            expected_data: json!({ CHUNKS: [
                { "text": FIRST },
                { "text": format!("It ends at sea.\n\n{SECOND}") },
                { "text": format!("It ends in town.\n\n{THIRD}") },
            ] }),
            common,
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
