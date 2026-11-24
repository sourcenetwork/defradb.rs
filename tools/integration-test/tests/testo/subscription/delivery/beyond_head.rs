//! A cursor past the collection's head is refused, never answered with an
//! empty page (`defra.events.delivery.beyond-head`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.events.delivery.beyond-head")]
fn a_cursor_past_the_head_is_refused() {
    let mut case = TestCase::from_actions(vec![
        AddCollection {
            sdl: "type User { name: String }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "Ann" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query { _documentUpdates(collection: "User", after: "1") { head } }"#
                .into(),
            expected_error: Some("update cursor is beyond this node's collection head".into()),
            ..Default::default()
        }
        .into(),
    ]);
    case.id = integration_test::scenario_id!();
    // `_documentUpdates` is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}
