//! Reads that race the commit of a migration set in an explicit transaction
//! never miss the migration's transform.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, ConfigureMigration, Parallel, PatchCollection, Request,
    TxnBegin, TxnCommit,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

const DOCS: usize = 4;
const READS: usize = 16;

fn users(verified: serde_json::Value) -> serde_json::Value {
    json!({ "Users": vec![json!({ "verified": verified }); DOCS] })
}

fn read(expected: serde_json::Value) -> Actions {
    Request {
        request: "query { Users { verified } }".into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn collection_migration_reads_during_a_txn_migration_commit_find_the_transform() {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: r#"
            type Users {
                name: String
            }
        "#
        .into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..DOCS).map(|i| {
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": format!("u{i}") })),
            ..Default::default()
        }
        .into()
    }));
    actions.push(
        PatchCollection {
            patch: r#"
                [
                    { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
                ]
            "#
            .into(),
            ..Default::default()
        }
        .into(),
    );
    actions.push(TxnBegin::default().into());
    actions.push(
        ConfigureMigration {
            txn: Some(0),
            source_version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu".into(),
            destination_version_id: "bafyreigqfjat435ghyt66tdaucp7oi2mke5jafx3jw3rozanopihr2vf44"
                .into(),
            lens: Lens {
                lenses: Some(vec![LensModule {
                    arguments: json!({ "dst": "verified", "value": true }),
                    path: "bin/lenses/set-default.wasm".into(),
                }]),
            },
            ..Default::default()
        }
        .into(),
    );
    // Each read sees the migration committed or not, never a migration whose
    // transform is missing.
    let before_or_after = m::any_of(json!([users(json!(null)), users(json!(true))]));
    actions.push(
        Parallel {
            actions: std::iter::once(
                TxnCommit {
                    txn: 0,
                    ..Default::default()
                }
                .into(),
            )
            .chain((0..READS).map(|_| read(before_or_after.clone())))
            .collect(),
            ..Default::default()
        }
        .into(),
    );
    actions.push(read(users(json!(true))));
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}
