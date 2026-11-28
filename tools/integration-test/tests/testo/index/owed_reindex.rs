//! A migration's reindex that stops part way stays owed: the next open
//! settles it, and so does the next schema change. Where the open resumes
//! (after the last committed page) is pinned by
//! `db::index::reindex_resume`, since starting over builds the same index.
//!
//! The `increment` lens fails on the one document stored without `age`. It
//! is created last, so the rebuild commits its first pages before it reaches
//! that document, and purging it lets the rebuild through. Go's parity pin
//! has no document purge, so these run on the Rust clients only.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, NewNode, PatchCollection, PurgeDocs, Request, Restart,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};
use testo::action::ActionInstance;

const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

/// More documents than one rebuild page holds.
const USERS: usize = 300;

const FILTER_BY_AGE: &str = "query { Users(filter: {age: {_eq: 25}}) { name age } }";

const EXPLAIN_FILTER_BY_AGE: &str =
    "query @explain(type: execute) { Users(filter: {age: {_eq: 25}}) { name } }";

const FILTER_FROM_FIVE: &str = "query { Users(filter: {age: {_ge: 5}}) { age } }";

const EXPLAIN_FILTER_FROM_FIVE: &str =
    "query @explain(type: execute) { Users(filter: {age: {_ge: 5}}) { age } }";

fn users() -> Vec<Value> {
    (0..USERS)
        .map(|user| json!({ "name": format!("u{user:03}"), "age": user }))
        .collect()
}

/// A collection indexed on `age`, its users, the last one without an `age`,
/// and a migration whose lens adds 5 to `age`, which the rebuild fails on at
/// that last user, who is then purged.
fn stopped_rebuild() -> Vec<ActionInstance<integration_test::actions::Actions>> {
    vec![
        NewNode {
            database_dir: Some("".into()),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type Users { name: String age: Int @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            docs: Some(users()),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "ageless" })),
            common: Common {
                capture: captures([("ageless", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch:
                r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#
                    .into(),
            lens: Some(Lens {
                lenses: Some(vec![LensModule {
                    arguments: json!({ "field": "age", "value": 5 }),
                    path: "bin/lenses/increment.wasm".into(),
                }]),
            }),
            ..Default::default()
        }
        .into(),
        PurgeDocs {
            collection: "Users".into(),
            doc_ids: vec![Text::reference("ageless")],
            expected_count: Some(1),
            ..Default::default()
        }
        .into(),
    ]
}

/// While the reindex is owed, the index answers no query: the filter scans,
/// and returns the migrated value.
fn owed() -> Vec<ActionInstance<integration_test::actions::Actions>> {
    vec![
        Request {
            request: EXPLAIN_FILTER_BY_AGE.into(),
            expected_data: m::explain_scan(json!({ "indexFetches": 0 })),
            ..Default::default()
        }
        .into(),
        Request {
            request: FILTER_BY_AGE.into(),
            expected_data: json!({ "Users": [{ "name": "u020", "age": 25 }] }),
            ..Default::default()
        }
        .into(),
    ]
}

/// Once the reindex settles, the index answers the filter with the migrated
/// value of every document. The range is read from the index, so a document
/// the rebuild skipped is missing from it rather than found by a scan.
fn settled() -> Vec<ActionInstance<integration_test::actions::Actions>> {
    vec![
        Request {
            request: EXPLAIN_FILTER_BY_AGE.into(),
            expected_data: m::explain_scan(json!({ "indexFetches": 1 })),
            ..Default::default()
        }
        .into(),
        Request {
            request: FILTER_BY_AGE.into(),
            expected_data: json!({ "Users": [{ "name": "u020", "age": 25 }] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: EXPLAIN_FILTER_FROM_FIVE.into(),
            expected_data: m::explain_scan(json!({ "indexFetches": USERS })),
            ..Default::default()
        }
        .into(),
        Request {
            request: FILTER_FROM_FIVE.into(),
            expected_data: json!({
                "Users": m::unordered(json!(
                    (0..USERS).map(|user| json!({ "age": user + 5 })).collect::<Vec<_>>()
                )),
            }),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.index.owed_reindex.resume")]
fn a_stopped_rebuild_stays_owed_and_the_next_open_settles_it() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(RUST_CLIENTS),
        actions: [
            stopped_rebuild(),
            owed(),
            vec![Restart::default().into()],
            settled(),
        ]
        .concat(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.owed_reindex.resume")]
fn a_stopped_rebuild_stays_owed_and_the_next_schema_change_settles_it() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(RUST_CLIENTS),
        actions: [
            stopped_rebuild(),
            owed(),
            vec![PatchCollection {
                patch: r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"rank","Kind":"Int"}}]"#
                    .into(),
                ..Default::default()
            }
            .into()],
            settled(),
        ]
        .concat(),
        ..Default::default()
    }
    .execute();
}
