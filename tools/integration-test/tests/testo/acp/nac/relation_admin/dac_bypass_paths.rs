//! A NAC admin bypasses document access control on every path, not only on a
//! plain query: commits, an aggregate, a subscription's events, and the
//! documents an update or a delete changes. An identity NAC lets read,
//! update and delete documents, but not bypass DAC, gets on those same paths
//! only what DAC grants it.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, AddNACActorRelationship, Close, Request, Start,
    SubscriptionRequest,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const POLICY: &str = r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: reader
    name: delete
  - expr: reader
    name: read
  - expr: reader
    name: update
  relations:
  - manages:
    - reader
    name: manager
    types:
    - actor
  - name: reader
    types:
    - actor
"#;

fn seed(identity: u64) -> Common {
    Common {
        identity_seed: Some(identity),
        ..Default::default()
    }
}

/// The identity NAC lets read, update and delete documents, but not bypass DAC.
const READER: u64 = 3;

/// `READER` granted the node permission `relation`.
fn grant(relation: &str) -> AddNACActorRelationship {
    AddNACActorRelationship {
        relation: relation.into(),
        target_identity_seed: Some(READER),
        expected_data: json!({ "existedAlready": false }),
        common: seed(1),
        ..Default::default()
    }
}

fn request(identity: u64, request: Text, expected_data: Value) -> Request {
    Request {
        request,
        expected_data,
        common: seed(identity),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.acp.nac_admin_dac_bypass_paths")]
fn a_nac_admin_bypasses_dac_on_commits_aggregates_subscriptions_and_writes() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: seed(1),
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: POLICY.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(r#"type Users @policy(id: ""#),
                    Text::reference("policy0"),
                    Text::from(r#"", resource: "users") { name: String }"#),
                ]),
                common: seed(1),
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "existedAlready": false }),
                common: seed(1),
                ..Default::default()
            }
            .into(),
            grant("get-collection").into(),
            grant("read-document").into(),
            grant("update-document").into(),
            grant("delete-document").into(),
            SubscriptionRequest {
                subscription: "admin".into(),
                request: Some("subscription { Users { name } }".into()),
                expected_count: Some(1),
                common: seed(2),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "reader".into(),
                request: Some("subscription { Users { name } }".into()),
                common: seed(READER),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "admin".into(),
                expected_data: json!({
                    "events": [{ "data": { "Users": [{ "name": "Shahzad" }] } }],
                }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "reader".into(),
                expected_data: json!({ "events": [] }),
                ..Default::default()
            }
            .into(),
            request(
                READER,
                "query { Users { name } }".into(),
                json!({ "Users": [] }),
            )
            .into(),
            request(
                READER,
                "query { _commits { height } }".into(),
                json!({ "_commits": [] }),
            )
            .into(),
            request(
                READER,
                "query { COUNT(Users: {}) }".into(),
                json!({ "COUNT": 0 }),
            )
            .into(),
            request(
                READER,
                r#"mutation { update_Users(filter: {}, input: {name: "Bob"}) { name } }"#.into(),
                json!({ "update_Users": [] }),
            )
            .into(),
            request(
                READER,
                Text::concat([
                    Text::from(r#"mutation { delete_Users(docID: ""#),
                    Text::reference("doc0"),
                    Text::from(r#"") { _docID } }"#),
                ]),
                json!({ "delete_Users": [] }),
            )
            .into(),
            request(
                1,
                "query { Users { name } }".into(),
                json!({ "Users": [{ "name": "Shahzad" }] }),
            )
            .into(),
            request(
                2,
                "query { Users { name } }".into(),
                json!({ "Users": [{ "name": "Shahzad" }] }),
            )
            .into(),
            request(
                2,
                "query { _commits { height } }".into(),
                json!({ "_commits": [{ "height": 1 }, { "height": 1 }] }),
            )
            .into(),
            request(
                2,
                "query { COUNT(Users: {}) }".into(),
                json!({ "COUNT": 1 }),
            )
            .into(),
            request(
                2,
                r#"mutation { update_Users(filter: {}, input: {name: "Bob"}) { name } }"#.into(),
                json!({ "update_Users": [{ "name": "Bob" }] }),
            )
            .into(),
            request(
                2,
                Text::concat([
                    Text::from(r#"mutation { delete_Users(docID: ""#),
                    Text::reference("doc0"),
                    Text::from(r#"") { _docID } }"#),
                ]),
                json!({ "delete_Users": [{ "_docID": m::r("doc0") }] }),
            )
            .into(),
            request(1, "query { Users { name } }".into(), json!({ "Users": [] })).into(),
        ],
        ..Default::default()
    }
    .execute();
}
