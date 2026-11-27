//! A dump lists the node's store through every Rust client.

use integration_test::actions::{AddCollection, AddDoc, Close, PrintDump, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const NOT_AUTHORIZED: &str = "not authorized to perform operation. Permission: read-document";

fn as_seed(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

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

#[test]
#[mantra_macros::req_test("defra.backup.dump")]
fn a_dump_by_another_identity_is_not_authorized() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: as_seed(1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String }".into(),
                common: as_seed(1),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: as_seed(1),
                ..Default::default()
            }
            .into(),
            PrintDump {
                expected_error: Some(NOT_AUTHORIZED.into()),
                common: as_seed(2),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
