//! Introspection describes the subscriptions served over SSE, as Go does
//! (#2649, `defra.request.subscription_introspection`).

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

fn names(items: &[&str]) -> serde_json::Value {
    json!(items
        .iter()
        .map(|name| json!({ "name": name }))
        .collect::<Vec<_>>())
}

#[test]
#[mantra_macros::req_test("defra.request.subscription_introspection")]
fn subscriptions_appear_in_introspection() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { name: String }".into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query {
                __schema { subscriptionType { name } }
                subscription: __type(name: "Subscription") { fields { name type { kind ofType { name } } args { name } } }
            }"#
            .into(),
            expected_data: json!({
                "__schema": { "subscriptionType": { "name": "Subscription" } },
                "subscription": { "fields": m::unordered(json!([
                    {
                        "name": "Book",
                        "type": { "kind": "LIST", "ofType": { "name": "Book" } },
                        "args": names(&["cid", "docID", "filter", "groupBy", "limit", "offset", "order", "showDeleted"]),
                    },
                    {
                        "name": "_commits",
                        "type": { "kind": "LIST", "ofType": { "name": "Commit" } },
                        "args": names(&["cid", "depth", "docID", "filter", "groupBy", "limit", "offset", "order"]),
                    },
                ])) },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
