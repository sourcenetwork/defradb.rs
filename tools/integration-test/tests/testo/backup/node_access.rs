//! A backup export needs the `DocumentRead` node permission, whichever edge
//! it comes in by.

use integration_test::actions::{AddCollection, AddDoc, Close, ExportBackup, Start};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

const NOT_AUTHORIZED: &str = "not authorized to perform operation. Permission: read-document";

fn as_seed(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

fn export_as(identity_seed: Option<u64>, expected_error: Option<&str>) -> ExportBackup {
    let expected_data = match expected_error {
        Some(_) => serde_json::Value::Null,
        None => json!({
            "content": m::concat([
                json!(r#"{"User":[{"_docID":""#),
                m::r("doc0_0"),
                json!(r#"","_docIDNew":""#),
                m::r("doc0_0"),
                json!(r#"","name":"Shahzad"}]}"#),
            ]),
        }),
    };
    ExportBackup {
        expected_data,
        expected_error: expected_error.map(Into::into),
        common: Common {
            identity_seed,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// On a node with node ACP whose owner is identity 1, the owner adds a
/// collection and a document, then `export` runs.
fn with_nac_and_a_document(id: &'static str, export: ExportBackup) -> TestCase {
    TestCase {
        id,
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
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..as_seed(1)
                },
                ..Default::default()
            }
            .into(),
            export.into(),
        ],
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.backup.export.node_access")]
fn an_export_by_the_node_owner_returns_the_documents() {
    with_nac_and_a_document(integration_test::scenario_id!(), export_as(Some(1), None)).execute();
}

#[test]
#[mantra_macros::req_test("defra.backup.export.node_access")]
fn an_export_by_another_identity_is_not_authorized() {
    with_nac_and_a_document(
        integration_test::scenario_id!(),
        export_as(Some(2), Some(NOT_AUTHORIZED)),
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.backup.export.node_access")]
fn an_export_with_no_identity_is_not_authorized() {
    with_nac_and_a_document(
        integration_test::scenario_id!(),
        export_as(None, Some(NOT_AUTHORIZED)),
    )
    .execute();
}
