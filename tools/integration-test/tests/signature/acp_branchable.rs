//! Generated from the conformance scenarios of tests/integration/signature/acp_branchable_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, GetCollections, NewNode, VerifyBlockSignature,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.signature.verify.acp")]
fn signature_acp_with_branchable_collection_if_has_collection_access_validate_signature() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: test
description: a test policy which marks a collection in a database as a resource

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
                    Text::from(
                        r#"
	type Users @branchable @policy(
		id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
		resource: "users"
	) {
		name: String
		age: Int
	}
"#,
                    ),
                ]),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc"), Text::reference("collection")]),
                common: Common {
                    capture: captures([("head", "/cids/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_seed: Some(1),
                cid: "bafyreia7dgbk4gjeo3hvdqcbizzl4f25onmy7oetwhvxe6sq3lup5n7xju".into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.signature.verify.acp")]
fn signature_acp_with_branchable_collection_if_has_no_collection_access_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: test
description: a test policy which marks a collection in a database as a resource

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
                    Text::from(
                        r#"
	type Users @branchable @policy(
		id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
		resource: "users"
	) {
		name: String
		age: Int
	}
"#,
                    ),
                ]),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc"), Text::reference("collection")]),
                common: Common {
                    capture: captures([("head", "/cids/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_seed: Some(1),
                cid: "bafyreia7dgbk4gjeo3hvdqcbizzl4f25onmy7oetwhvxe6sq3lup5n7xju".into(),
                expected_error: Some("missing permission".into()),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
