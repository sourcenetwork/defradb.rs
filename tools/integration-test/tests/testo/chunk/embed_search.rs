//! Passage search: the chunk collection ranks chunks across articles by
//! similarity, and shows only chunks of articles the reader may read
//! (`defra.chunk.embed.search`, `defra.chunk.embed.search_access`).

use integration_test::actions::{AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::{json, Value};

use super::access_support::{ALICE, BOB};
use super::embed_support::{add_embedded_collection, embedded_schema, model_embedded, paragraph};
use super::support::{body, CHUNKS};

const PARAGRAPHS: usize = 2;
const TOP: usize = 2;

/// The `TOP` chunks nearest to "rivers", with their article's title.
fn nearest_to_rivers(expected: Value, identity: Option<u64>) -> Request {
    Request {
        request: format!(
            "query {{ {CHUNKS}(order: {{_alias: {{sim: DESC}}}}, limit: {TOP}) {{
                text
                sim: SIMILARITY(body_v: {{vector: [1, 0, 0]}})
                source {{ title }}
            }} }}"
        )
        .into(),
        expected_data: expected,
        common: Common {
            identity_seed: identity,
            unordered: vec![format!("/data/{CHUNKS}")],
            ..Default::default()
        },
        ..Default::default()
    }
}

fn river_chunks(title: &str) -> Vec<Value> {
    (0..PARAGRAPHS)
        .map(|i| {
            json!({
                "text": paragraph(i, "rivers", i + 1 == PARAGRAPHS),
                "sim": 1.0,
                "source": { "title": title },
            })
        })
        .collect()
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.search")]
fn passage_search_ranks_chunks_across_articles() {
    let mut actions = vec![model_embedded(&[]), add_embedded_collection(None)];
    for topic in ["engines", "rivers", "music"] {
        actions.push(
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({ "title": topic, "body": body(PARAGRAPHS, topic) })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(nearest_to_rivers(json!({ CHUNKS: river_chunks("rivers") }), None).into());
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.search_access")]
fn passage_search_shows_only_chunks_of_readable_articles() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
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
        integration_test::actions::AddCollection {
            sdl: embedded_schema(Some("policy")),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "secret", "body": body(PARAGRAPHS, "rivers") })),
            common: Common {
                identity_seed: Some(ALICE),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        nearest_to_rivers(json!({ CHUNKS: river_chunks("secret") }), Some(ALICE)).into(),
        nearest_to_rivers(json!({ CHUNKS: [] }), Some(BOB)).into(),
        nearest_to_rivers(json!({ CHUNKS: [] }), None).into(),
    ])
    .execute();
}
