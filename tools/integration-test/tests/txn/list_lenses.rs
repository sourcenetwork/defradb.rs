//! Generated from the conformance scenarios of tests/integration/txn/list_lenses_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddLens, ListLenses, TxnBegin, TxnDiscard};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.definitions.same-txn-visibility")]
fn txn_list_lenses_inside_txn_with_add_lens_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddLens {
                txn: Some(0),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "fullName", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListLenses {
                txn: Some(0),
                expected_data: json!({
                    "lenses": [
                        {
                            "lensID": m::r("lens0"),
                            "modules": [
                                {
                                    "Inverse": false,
                                    "Arguments": { "dst": "fullName", "src": "name" },
                                },
                            ],
                        },
                    ],
                }),
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

#[test]
#[mantra_macros::req_test("go.txn.definitions.same-txn-visibility")]
fn txn_list_lenses_inside_txn_without_add_lens_no_lenses() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddLens {
                txn: Some(0),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "fullName", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            ListLenses {
                txn: Some(1),
                expected_data: json!({ "lenses": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
