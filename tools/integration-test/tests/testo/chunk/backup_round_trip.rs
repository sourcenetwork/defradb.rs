//! A backup leaves chunks out, and importing it cuts the texts again (`defra.chunk.backup.round_trip`).

use integration_test::actions::{AddCollection, ExportBackup, ImportBackup, NewNode};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_on, body, chunks_are_listed, list_is, on, read_list, SCHEMA};

fn node(name: &str) -> NewNode {
    NewNode {
        common: on(Some(name)),
        ..Default::default()
    }
}

fn article_collection(name: &str) -> AddCollection {
    AddCollection {
        sdl: SCHEMA.into(),
        common: on(Some(name)),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.backup.round_trip")]
fn a_backup_round_trip_rebuilds_the_chunks() {
    let text = body(5, "harbours");
    let escaped = serde_json::to_string(&text).expect("a JSON string");
    let mut case = TestCase::from_actions(vec![
        node("db0").into(),
        node("db1").into(),
        article_collection("db0").into(),
        article_collection("db1").into(),
        add_article_on(&text, "article", Some("db0")),
        read_list("source", 5, Some("db0")),
        ExportBackup {
            expected_data: json!({
                "content": m::concat([
                    json!(r#"{"Article":[{"_docID":""#),
                    m::r("article"),
                    json!(r#"","_docIDNew":""#),
                    m::r("article"),
                    json!(format!(r#"","body":{escaped},"title":"t"}}]}}"#)),
                ]),
            }),
            common: Common {
                node: Some("db0".into()),
                capture: captures([("backup", "/content")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        ImportBackup {
            import_content: Text::reference("backup"),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        list_is("source", 5, Some("db1")),
        chunks_are_listed("source", 5, Some("db1")),
    ]);
    case.without_db = true;
    case.execute();
}
