//! The CLI reads a remote-DAC node's status with the caller's identity token
//! and signs a Vera delegation with an Ed25519 identity.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, NewNode, Start,
};
use integration_test::m;
use integration_test::scenario::identity::ED25519_IDENTITY_BASE;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const OWNER: u64 = ED25519_IDENTITY_BASE + 2;
const READER: u64 = 3;
const CLIENTS: &[ClientType] = &[ClientType::Cli];

fn as_owner(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        identity_seed: Some(OWNER),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.cli.identity.vera_delegation")]
fn a_cli_ed25519_identity_grants_under_nac_and_reads_dac_status() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                is_document_acp_test: true,
                common: Common {
                    node: Some("service".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(0),
                with_identity: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: as_owner("db0"),
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: deleter
    name: delete
  - expr: reader
    name: read
  - expr: updater
    name: update
  relations:
  - manages:
    - deleter
    - reader
    - updater
    name: admin
    types:
    - actor
  - name: deleter
    types:
    - actor
  - name: reader
    types:
    - actor
  - name: updater
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    capture: captures([("policy", "/policyID")]),
                    ..as_owner("db0")
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from("type Users @policy(id: \""),
                    Text::reference("policy"),
                    Text::from("\", resource: \"users\") { name: String }"),
                ]),
                common: as_owner("db0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    retry: true,
                    ..as_owner("db0")
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                relation: "reader".into(),
                target_identity_seed: Some(READER),
                expected_data: json!({ "existedAlready": false }),
                common: as_owner("db0"),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        without_db: true,
        ..Default::default()
    }
    .execute();
}
