//! Generated from the conformance scenarios of tests/integration/collection_version/updates/remove/view_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddView, PatchCollection, Request, TxnBegin, TxnDiscard,
};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_view_does_not_deadlock_if_deleting_version_with_no_new_fields_whilst_other_txn_reading(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Users {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: true) {
                        name: String
                        fullName: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/UserView/Query/Query",
							"value": {"Name": "Users", "Fields":[{"Name":"name","Alias":"fullName"}]}
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                                        UserView {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "UserView": [] }),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/bafyreihekbkpnimeo5g5tkv5a3urtcalx2qd447tyhhptjunb7vvpdyvue"
						}
					]
				"#
                .into(),
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
