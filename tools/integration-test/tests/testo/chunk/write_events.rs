//! A parent write publishes an event for each chunk it creates (`defra.chunk.write.events`).

use integration_test::actions::SubscriptionRequest;
use integration_test::m;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::support::{add_article, add_article_collection, body, texts, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.write.events")]
fn a_parent_create_publishes_its_chunks() {
    let text = body(3, "rivers");
    let events: Vec<_> = texts(&text)
        .into_iter()
        .map(|text| json!({ "data": { CHUNKS: [{ "text": text }] } }))
        .collect();
    let mut case = TestCase::from_actions(vec![
        add_article_collection(),
        SubscriptionRequest {
            subscription: "chunks".into(),
            request: Some(format!("subscription {{ {CHUNKS} {{ text }} }}").into()),
            expected_count: Some(3),
            ..Default::default()
        }
        .into(),
        add_article(&text, "article"),
        SubscriptionRequest {
            subscription: "chunks".into(),
            expected_data: json!({ "events": m::unordered(json!(events)) }),
            ..Default::default()
        }
        .into(),
    ]);
    // The CLI has no subscription stream.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp]);
    case.execute();
}
