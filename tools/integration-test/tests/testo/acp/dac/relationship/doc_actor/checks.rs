//! A relationship change is checked the same through every client, the
//! `defradb start` server included: a collection without a policy and a
//! relation the policy does not have are refused with Go's errors.

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc,
    DeleteDACActorRelationship, TxnBegin, TxnDiscard,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

const POLICY: &str = r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: reader
    name: read
  - name: update
  - name: delete
  relations:
  - name: reader
    types:
    - actor
"#;

/// A collection with the policy `POLICY` names, and one document the
/// identity of seed 1 owns, captured as `doc`.
fn policied_document() -> Vec<Action> {
    vec![
        AddDACPolicy {
            policy: POLICY.into(),
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
                Text::from(r#"type Users @policy(id: ""#),
                Text::reference("policy"),
                Text::from(r#"", resource: "users") { name: String }"#),
            ]),
            ..Default::default()
        }
        .into(),
        owned_document(),
    ]
}

/// A document of `Users` the identity of seed 1 creates, captured as `doc`.
fn owned_document() -> Action {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": "Shahzad" })),
        common: Common {
            identity_seed: Some(1),
            capture: captures([("doc", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// A collection with no policy holding one document, captured as `doc`.
fn unpolicied_document() -> Vec<Action> {
    vec![
        AddCollection {
            sdl: Text::from("type Users { name: String }"),
            ..Default::default()
        }
        .into(),
        owned_document(),
    ]
}

fn add(relation: &str, expected_error: &str) -> Action {
    AddDACActorRelationship {
        target_identity_seed: Some(2),
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        relation: relation.into(),
        expected_error: Some(expected_error.into()),
        common: Common {
            identity_seed: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn add_in_txn(relation: &str, expected_error: &str) -> Action {
    AddDACActorRelationship {
        target_identity_seed: Some(2),
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        relation: relation.into(),
        txn: Some(0),
        expected_error: Some(expected_error.into()),
        common: Common {
            identity_seed: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn begin() -> Action {
    TxnBegin {
        common: Common {
            identity_seed: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn discard() -> Action {
    TxnDiscard {
        txn: 0,
        common: Common {
            identity_seed: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn delete(relation: &str, expected_error: &str) -> Action {
    DeleteDACActorRelationship {
        target_identity_seed: Some(2),
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        relation: relation.into(),
        expected_error: Some(expected_error.into()),
        common: Common {
            identity_seed: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn adding_on_a_collection_without_a_policy_is_refused_with_gos_error() {
    let mut actions = unpolicied_document();
    actions.push(add(
        "reader",
        "operation requires ACP, but collection has no policy",
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn deleting_on_a_collection_without_a_policy_is_refused_with_gos_error() {
    let mut actions = unpolicied_document();
    actions.push(delete(
        "reader",
        "operation requires ACP, but collection has no policy",
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn adding_a_relation_the_policy_lacks_is_refused_as_an_acp_failure() {
    let mut actions = policied_document();
    actions.push(add(
        "NotOnPolicy",
        "failed to add document actor relationship with acp",
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn deleting_a_relation_the_policy_lacks_is_refused_as_an_acp_failure() {
    let mut actions = policied_document();
    actions.push(delete(
        "NotOnPolicy",
        "failed to delete document actor relationship with acp",
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn a_transaction_uses_gos_check_order_before_the_owner_relation() {
    let mut actions = unpolicied_document();
    actions.extend([
        begin(),
        add_in_txn(
            "owner",
            "operation requires ACP, but collection has no policy",
        ),
        discard(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.dac.relationship_checks")]
fn a_transaction_wraps_a_policy_refusal_as_autocommit_does() {
    let mut actions = policied_document();
    actions.extend([begin(), add_in_txn(
        "NotOnPolicy",
        "failed to add document actor relationship with acp: relation 'NotOnPolicy' not found in policy resource 'users'",
    ), discard()]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
