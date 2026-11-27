//! Who may prune a collection's history, and where: the node permission of a
//! collection truncate, checked before the collection is looked up, and never
//! inside an explicit transaction.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, Close, PruneHistory, Start, TxnBegin, TxnDiscard,
};
use integration_test::scenario::Common;
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
