//! A chunk event reaches only readers of its parent (`defra.chunk.access.events`).

use integration_test::actions::{Actions, AddDoc, SubscriptionRequest};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

use super::access_support::{alice_owns_an_article, ALICE, BOB};
use super::support::{body, texts, CHUNKS};

fn subscribe(name: &str, identity: Option<u64>, count: Option<i64>) -> Actions {
    SubscriptionRequest {
        subscription: name.into(),
        request: Some(format!("subscription {{ {CHUNKS} {{ text }} }}").into()),
        expected_count: count,
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn received(name: &str, bodies: &[&str]) -> Actions {
    let events: Vec<Value> = bodies
        .iter()
        .flat_map(|body| texts(body))
        .map(|text| json!({ "data": { CHUNKS: [{ "text": text }] } }))
        .collect();
    SubscriptionRequest {
        subscription: name.into(),
        expected_data: json!({ "events": m::unordered(json!(events)) }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.access.events")]
fn chunk_events_reach_only_readers_of_the_parent() {
    let alice_body = body(super::access_support::PARAGRAPHS, "plans");
    let bob_body = body(2, "notes");
    let mut actions = alice_owns_an_article();
    // Opened once the collection exists and before Alice's article, which
    // comes first, so a leaked chunk would be among Bob's first events.
    actions.splice(
        2..2,
        [
            subscribe("alice", Some(ALICE), Some(5)),
            subscribe("bob", Some(BOB), Some(2)),
            subscribe("nobody", None, None),
        ],
    );
    actions.extend([
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "mine", "body": bob_body })),
            common: Common {
                identity_seed: Some(BOB),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        received("alice", &[&alice_body]),
        received("bob", &[&bob_body]),
        received("nobody", &[]),
    ]);
    let mut case = TestCase::from_actions(actions);
    // The CLI has no subscription stream.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp]);
    case.execute();
}
