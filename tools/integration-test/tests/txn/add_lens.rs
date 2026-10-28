//! Generated from the conformance scenarios of tests/integration/txn/add_lens_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddLens, AddView, Request, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.definitions.commit-visible")]
fn txn_add_lens_with_commit_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
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
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "fullName", "src": "name" }),
                            path: "bin/lenses/copy.wasm".into(),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens0", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#.into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        fullName: String
                    }
                "#.into(),
                transform_cid: Some(Text::reference("lens0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            fullName
                        }
                    }
                "#.into(),
                expected_data: json!({ "UserView": [{ "fullName": "John" }, { "fullName": "Fred" }] }),
                common: Common { unordered_documents: vec!["/data/UserView".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.txn.definitions.uncommitted-invisible")]
fn txn_add_lens_without_commit_fails() {
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
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        fullName: String
                    }
                "#
                .into(),
                transform_cid: Some(Text::reference("lens0")),
                expected_error: Some("lens CID not found".into()),
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
