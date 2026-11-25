//! A deleted document's retained history keeps its access control
//! (`defra.acp.deleted_history`): the commits and the signed blocks of a
//! deleted document stay hidden from an actor that could not read it, stay
//! readable by one that could, and a later revocation takes effect.

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc,
    DeleteDACActorRelationship, DeleteDoc, NewNode, PurgeDocs, Request, Restart, TxnBegin,
    TxnCommit, VerifyBlockSignature,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const OWNER: u64 = 1;
const READER: u64 = 2;
const FOREIGN: u64 = 3;

const POLICY: &str = r#"
name: test
description: a policy with a reader relation the owner manages

resources:
- name: users
  permissions:
  - name: read
    expr: reader
  - name: update
  - name: delete

  relations:
  - name: reader
    types:
    - actor

  - name: admin
    manages:
    - reader
    types:
    - actor
"#;

fn as_seed(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

/// A signing node with the `Users` collection under [`POLICY`] and one
/// document the owner added, captured as `doc`, whose composite head is
/// captured as `head`.
fn protected_document() -> Vec<Actions> {
    vec![
        NewNode {
            enable_signing: true,
            ..Default::default()
        }
        .into(),
        AddDACPolicy {
            policy: POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(OWNER),
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
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "secret" })),
            common: Common {
                identity_seed: Some(OWNER),
                capture: captures([("doc", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: commits_query(r#", filter: {fieldName: {_eq: "_C"}}"#),
            expected_data: json!({ "_commits": [{ "cid": m::cid() }] }),
            common: Common {
                identity_seed: Some(OWNER),
                capture: captures([("head", "/data/_commits/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn commits_query(extra: &str) -> Text {
    Text::concat([
        Text::from(r#"query { _commits(docID: ""#),
        Text::reference("doc"),
        Text::from(format!(r#""{extra}) {{ cid }} }}"#)),
    ])
}

fn commits(seed: u64, expected: Value) -> Actions {
    Request {
        request: commits_query(""),
        expected_data: json!({ "_commits": expected }),
        common: as_seed(seed),
        ..Default::default()
    }
    .into()
}

fn head_block(seed: u64, expected_error: Option<&str>) -> Actions {
    VerifyBlockSignature {
        signer_seed: Some(OWNER),
        cid: Text::reference("head"),
        expected_error: expected_error.map(str::to_owned),
        common: as_seed(seed),
        ..Default::default()
    }
    .into()
}

fn delete(txn: Option<usize>) -> Actions {
    DeleteDoc {
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        txn,
        common: as_seed(OWNER),
        ..Default::default()
    }
    .into()
}

fn grant_reader() -> Actions {
    AddDACActorRelationship {
        collection: "Users".into(),
        doc_id: Text::reference("doc"),
        relation: "reader".into(),
        target_identity_seed: Some(READER),
        expected_data: json!({ "existedAlready": false }),
        common: as_seed(OWNER),
        ..Default::default()
    }
    .into()
}

fn denied(seed: u64) -> [Actions; 2] {
    [
        commits(seed, json!([])),
        head_block(seed, Some("missing permission")),
    ]
}

fn allowed(seed: u64) -> [Actions; 2] {
    [commits(seed, m::nonempty()), head_block(seed, None)]
}

fn run(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        client_types: Some(&[ClientType::InProcess]),
        ..TestCase::from_actions(actions)
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.deleted_history.hidden")]
fn a_deleted_documents_history_stays_hidden_from_an_actor_without_access() {
    let mut actions = protected_document();
    actions.extend(denied(FOREIGN));
    actions.push(delete(None));
    actions.extend(denied(FOREIGN));
    actions.extend(allowed(OWNER));
    actions.push(Restart::default().into());
    actions.extend(denied(FOREIGN));
    actions.extend(allowed(OWNER));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.deleted_history.hidden")]
fn a_delete_committed_in_a_transaction_keeps_the_history_hidden() {
    let mut actions = protected_document();
    actions.push(TxnBegin::default().into());
    actions.push(delete(Some(0)));
    actions.push(
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
    );
    actions.extend(denied(FOREIGN));
    actions.extend(allowed(OWNER));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.deleted_history.grants")]
fn a_reader_keeps_a_deleted_documents_history_until_revoked() {
    let mut actions = protected_document();
    actions.push(grant_reader());
    actions.push(delete(None));
    actions.extend(allowed(READER));
    actions.push(
        DeleteDACActorRelationship {
            collection: "Users".into(),
            doc_id: Text::reference("doc"),
            relation: "reader".into(),
            target_identity_seed: Some(READER),
            expected_data: json!({ "recordFound": true }),
            common: as_seed(OWNER),
            ..Default::default()
        }
        .into(),
    );
    actions.extend(denied(READER));
    actions.extend(allowed(OWNER));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.deleted_history.purge")]
fn a_purged_document_added_again_takes_no_earlier_grant() {
    let mut actions = protected_document();
    actions.push(grant_reader());
    actions.push(delete(None));
    actions.push(
        PurgeDocs {
            collection: "Users".into(),
            doc_ids: vec![Text::reference("doc")],
            expected_count: Some(1),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "secret" })),
            expected_data: json!({ "docID": m::r("doc") }),
            common: as_seed(OWNER),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        Request {
            request: "query { Users { name } }".into(),
            expected_data: json!({ "Users": [] }),
            common: as_seed(READER),
            ..Default::default()
        }
        .into(),
    );
    actions.push(commits(READER, json!([])));
    actions.push(
        Request {
            request: "query { Users { name } }".into(),
            expected_data: json!({ "Users": [{ "name": "secret" }] }),
            common: as_seed(OWNER),
            ..Default::default()
        }
        .into(),
    );
    run(integration_test::scenario_id!(), actions);
}
