//! Alice owns an article under a local policy; Bob is another actor.

use integration_test::actions::{Actions, AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::USER_ACP_POLICY;
use serde_json::{json, Value};

use super::support::{as_identity, body, chunks_are_listed, read_list};

pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const PARAGRAPHS: usize = 5;

fn schema() -> Text {
    Text::concat([
        Text::from(r#"type Article @policy(id: ""#),
        Text::reference("policy"),
        Text::from(
            r#"", resource: "users") {
                title: String
                body: String @chunk(maxChars: 64)
            }"#,
        ),
    ])
}

/// The policy, the collection, Alice's article `article` and the chunk
/// hashes she sees, named `plans0` to `plans5`.
pub fn alice_owns_an_article() -> Vec<Actions> {
    vec![
        AddDACPolicy {
            policy: USER_ACP_POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: schema(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "secret", "body": body(PARAGRAPHS, "plans") })),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("article", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        as_identity(read_list("plans", PARAGRAPHS, None), ALICE),
        as_identity(chunks_are_listed("plans", PARAGRAPHS, None), ALICE),
    ]
}

/// A read as `identity`, or as nobody, that must return `expected`.
pub fn read(request: Text, expected: Value, identity: Option<u64>) -> Actions {
    Request {
        request,
        expected_data: expected,
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// The same read as Alice, which returns `alice`, and as Bob and nobody, which return `none`.
pub fn seen_by_alice_only(request: &Text, alice: Value, none: Value) -> Vec<Actions> {
    vec![
        read(request.clone(), alice, Some(ALICE)),
        read(request.clone(), none.clone(), Some(BOB)),
        read(request.clone(), none, None),
    ]
}

/// Text built around the captured chunk docID `chunk`.
pub fn with_chunk_id(before: &str, after: &str) -> Text {
    Text::concat([
        Text::from(before),
        Text::reference("chunk"),
        Text::from(after),
    ])
}
