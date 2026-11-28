//! The `collection` section of a policy resource under the local ACP.

use integration_test::actions::{AddDACPolicy, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::common::{add_collection, add_policy, CLIENTS, LIBRARY};

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.policy_section")]
fn a_policy_with_a_collection_section_is_added() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_policy(LIBRARY, "library"),
            add_collection(
                r#"
                type Book @policy(id: "{library}", resource: "book") { title: String }
                "#,
                &["library"],
                None,
            ),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Dune" })),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { Book { title } }"#.into(),
                expected_data: json!({ "Book": [{ "title": "Dune" }] }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.acp.create_gate.policy_section")]
fn an_empty_collection_create_is_refused() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: LIBRARY.replace("create: author->publish", r#"create: """#),
            expected_error: Some("collection create of resource 'book' must not be empty".into()),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into()],
        client_types: Some(CLIENTS),
        ..Default::default()
    }
    .execute();
}
