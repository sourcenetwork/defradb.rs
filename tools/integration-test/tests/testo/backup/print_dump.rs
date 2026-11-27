//! A dump lists the node's store through every Rust client.

use integration_test::actions::{AddCollection, AddDoc, PrintDump};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.backup.dump")]
fn a_dump_lists_the_keys_of_the_store() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                ..Default::default()
            }
            .into(),
            PrintDump {
                expected_data: json!({ "listsKeys": true }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
