//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/is_active_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, GetCollections, PatchCollection, TxnBegin, TxnDiscard,
};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.active_version")]
fn col_version_update_replace_is_active_get_collections_with_txn() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                txn: Some(0),
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/IsActive",
							"value": false
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                txn: Some(0),
                collection_name: Some("Users".into()),
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            GetCollections {
                txn: Some(0),
                collection_id: Some(
                    "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna".into(),
                ),
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
