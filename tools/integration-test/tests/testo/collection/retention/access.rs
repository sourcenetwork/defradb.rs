//! Who may prune a collection's history, and where: the node permission of a
//! collection truncate, checked before the collection is looked up, and never
//! inside an explicit transaction.

use integration_test::actions::{
    Actions, AddCollection, AddDACPolicy, AddDoc, Close, PruneHistory, Request, Start, TxnBegin,
    TxnDiscard, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use serde_json::json;

use super::{add, collection, prune, rust_only, update, RETAINED};

fn as_seed(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

fn prune_as(seed: u64, collection: &str, expected_error: &str) -> Actions {
    PruneHistory {
        collection: collection.into(),
        expected_error: Some(expected_error.into()),
        common: as_seed(seed),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_prune_inside_a_transaction_is_refused() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        TxnBegin::default().into(),
        PruneHistory {
            collection: "User".into(),
            txn: Some(0),
            expected_error: Some("history prune cannot run in a transaction".into()),
            ..Default::default()
        }
        .into(),
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        prune(1),
    ])
    .execute();
}

/// An identity without the permission is denied whether or not the
/// collection exists, so a denial tells it nothing about the collections.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_prune_needs_the_truncate_permission() {
    rust_only(vec![
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
            sdl: RETAINED.into(),
            common: as_seed(1),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "John", "age": 21 })),
            common: as_seed(1),
            ..Default::default()
        }
        .into(),
        prune_as(2, "User", "not authorized to perform operation"),
        prune_as(2, "Nobody", "not authorized to perform operation"),
        prune_as(1, "Nobody", "collection 'Nobody' not found"),
        PruneHistory {
            collection: "User".into(),
            expected_count: Some(0),
            common: as_seed(1),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// A caller who cannot read a document gets the same empty result from a
/// pruned version as from a readable one; only a caller with read access gets
/// the error that names the pruned commit.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_pruned_version_read_checks_document_access_first() {
    let as_seed_1 = Common {
        identity_seed: Some(1),
        ..Default::default()
    };
    rust_only(vec![
        AddDACPolicy {
            policy: r#"
description: retention read policy
name: retention
resources:
- name: users
  permissions:
  - name: delete
  - name: read
    expr: reader
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
            .into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                Text::from("type User @retention(maxDepth: 1) @policy(id: \""),
                Text::reference("policy"),
                Text::from("\", resource: \"users\") { name: String age: Int }"),
            ]),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "John", "age": 21 })),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("john", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference("john"),
            doc_map: json!({ "age": 22 }),
            expected_data: json!({}),
            common: as_seed_1.clone(),
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference("john"),
            doc_map: json!({ "age": 23 }),
            expected_data: json!({}),
            common: as_seed_1,
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("john"),
                Text::from(
                    r#"", filter: {fieldName: {_eq: "_C"}}, order: {height: ASC}) { cid } }"#,
                ),
            ]),
            common: Common {
                identity_seed: Some(1),
                capture: captures([("old", "/data/_commits/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        prune(2),
        pruned_read(None, as_seed(2)),
        pruned_read(
            Some("its history was pruned to the collection's @retention depth"),
            as_seed(1),
        ),
    ])
    .execute();
}

fn pruned_read(error: Option<&str>, common: Common) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { User(cid: ""#),
            Text::reference("old"),
            Text::from(r#"") { name age } }"#),
        ]),
        expected_data: if error.is_some() {
            serde_json::Value::Null
        } else {
            json!({ "User": [] })
        },
        expected_error: error.map(Into::into),
        common,
        ..Default::default()
    }
    .into()
}
