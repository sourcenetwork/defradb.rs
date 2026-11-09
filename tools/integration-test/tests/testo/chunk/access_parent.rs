//! Under local ACP a chunk is shown only to readers of its parent (`defra.chunk.access.parent`).

use integration_test::actions::Request;
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::access_support::{
    alice_owns_an_article, read, seen_by_alice_only, with_chunk_id, ALICE, PARAGRAPHS,
};
use super::support::{as_identity, no_chunks, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.access.parent")]
fn chunks_follow_their_parents_access() {
    let n = PARAGRAPHS as u64;
    let commits = with_chunk_id(r#"query { _commits(docID: ""#, r#"") { cid } }"#);
    let mut actions = alice_owns_an_article();
    actions.extend([
        read(
            format!("query {{ {CHUNKS} {{ source {{ title }} }} }}").into(),
            json!({ CHUNKS: vec![json!({ "source": { "title": "secret" } }); PARAGRAPHS] }),
            Some(ALICE),
        ),
        as_identity(no_chunks(None), super::access_support::BOB),
        no_chunks(None),
        Request {
            request: format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID }} }}").into(),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("chunk", "/data/Article__body_chunk/0/_docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
    actions.extend(seen_by_alice_only(
        &commits,
        json!({ "_commits": m::nonempty() }),
        json!({ "_commits": [] }),
    ));
    actions.extend(seen_by_alice_only(
        &format!("query {{ COUNT({CHUNKS}: {{limit: 1000}}) }}").into(),
        json!({ "COUNT": n }),
        json!({ "COUNT": 0 }),
    ));
    actions.push(
        Request {
            request: commits,
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("cid", "/data/_commits/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    actions.extend(seen_by_alice_only(
        &Text::concat([
            Text::from(format!(r#"query {{ {CHUNKS}(cid: ""#)),
            Text::reference("cid"),
            Text::from(r#"") { text } }"#),
        ]),
        json!({ CHUNKS: m::nonempty() }),
        json!({ CHUNKS: [] }),
    ));
    actions.extend(seen_by_alice_only(
        &format!(r#"{{ _documentArrivals(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#)
            .into(),
        json!({ "_documentArrivals": { "entries": m::length(n) } }),
        json!({ "_documentArrivals": { "entries": [] } }),
    ));
    actions.extend(seen_by_alice_only(
        &format!("query {{ {CHUNKS} {{ _version {{ cid }} }} }}").into(),
        json!({ CHUNKS: m::length(n) }),
        json!({ CHUNKS: [] }),
    ));
    actions.push(read(
        format!("query @explain(type: execute) {{ {CHUNKS} {{ text }} }}").into(),
        json!({ "explain": m::object(json!({
            "executionSuccess": true,
            "operationNode": [m::object(json!({
                "selectTopNode": m::object(json!({ "permissionFilterNode": m::nonempty() })),
            }))],
        })) }),
        Some(super::access_support::BOB),
    ));
    TestCase::from_actions(actions).execute();
}
