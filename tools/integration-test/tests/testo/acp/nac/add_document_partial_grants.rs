//! Adding a document needs `get-collection` and `update-document`. A caller
//! holding only one of them is refused, naming the one it is missing, so the
//! HTTP edge deciding both in one check can't change which refusal comes back.

use integration_test::actions::{
    AddCollection, AddDoc, AddNACActorRelationship, Close, Start, WaitForPeersEvents,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

/// Scenario `id`: identity 2 holds only `granted`, then adds a document and
/// is refused naming `missing`.
fn refused_holding_only(id: &'static str, granted: &str, missing: &str) {
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String }".into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: granted.into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                expected_error: Some(format!(
                    "not authorized to perform operation. Permission: {missing}"
                )),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.documents")]
fn nac_gates_add_document_only_get_collection_refused_update_document() {
    refused_holding_only(
        integration_test::scenario_id!(),
        "get-collection",
        "update-document",
    );
}

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.documents")]
fn nac_gates_add_document_only_update_document_refused_get_collection() {
    refused_holding_only(
        integration_test::scenario_id!(),
        "update-document",
        "get-collection",
    );
}
