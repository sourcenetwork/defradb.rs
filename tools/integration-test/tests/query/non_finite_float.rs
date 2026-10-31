//! A Float that overflows to an infinity cannot be written as JSON. Every
//! client of a defradb.rs node gets Go's encoding error for it, never a
//! panic, a null or a body that does not parse. Go's own server writes an
//! empty body, which its HTTP client reports as "unexpected end of JSON
//! input" (defradb#2569), so the test does not run against it.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

fn overflow_then_query(start: f64, step: f64, expected_error: &str) {
    TestCase {
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String points: Float @crdt(type: pncounter) }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": start })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "points": step }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name points } }".into(),
                expected_error: Some(expected_error.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

#[test]
fn a_positive_infinity_is_refused_in_go_words() {
    overflow_then_query(f64::MAX, f64::MAX / 10.0, "json: unsupported value: +Inf");
}

#[test]
fn a_negative_infinity_is_refused_in_go_words() {
    overflow_then_query(-f64::MAX, -f64::MAX / 10.0, "json: unsupported value: -Inf");
}
