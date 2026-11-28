use integration_test::actions::{AddCollection, AddView, GetCollections};
use integration_test::test_case::TestCase;
use serde_json::json;

/// A view's `@policy` is checked as a collection's is: one naming a policy
/// the node does not hold is refused, and no view is left behind.
#[test]
#[mantra_macros::req_test("defra.view.add_checks")]
fn add_view_with_an_unknown_policy_fails_and_creates_no_view() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) @policy(
                        id: "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                        resource: "users"
                    ) {
                        name: String
                    }
                "#
                .into(),
                expected_error: Some("policyID specified does not exist with acp".into()),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("UserView".into()),
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
