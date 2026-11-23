//! An import that names no identity creates its documents as a create that
//! names no identity does on the same edge, so document ACP owns them alike.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, ImportBackup, NewNode, Request,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const POLICY: &str = r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#;

#[test]
#[mantra_macros::req_test("defra.backup.import.creator")]
fn an_anonymous_import_is_owned_as_an_anonymous_create() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                ..Default::default()
            }
            .into(),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Created" })),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"Users":[{"name":"Imported"}]}"#.into(),
                ..Default::default()
            }
            .into(),
            // Who owns an anonymous create depends on the client; the import
            // must land with the same owner, so another identity sees both
            // documents or neither.
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({
                    "Users": m::any_of(json!([
                        [],
                        m::unordered(json!([{ "name": "Created" }, { "name": "Imported" }])),
                    ])),
                }),
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
