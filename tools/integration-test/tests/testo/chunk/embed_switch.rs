use integration_test::actions::{
    Actions, AddCollection, AddDACPolicy, AddDoc, PatchCollection, Request, MODEL_CAPTURE,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::json;

use super::access_support::{ALICE, BOB};
use super::embed_failure::{model_fails, MODEL_FAILED};
use super::embed_support::{
    add_embedded_collection, chunks_have_vectors, chunks_of, embedded_schema, model_embedded,
};
use super::support::{add_article_on, body, CHUNKS};

fn initial() -> Vec<Actions> {
    vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_on(&body(1, "rivers"), "article", None),
    ]
}

fn switch(dimensions: u32, suffix: &str, expected_error: Option<&str>) -> PatchCollection {
    let prefix = format!(
        r#"[
            {{"op":"replace","path":"/{CHUNKS}/VectorEmbeddings/0/Provider","value":"ollama"}},
            {{"op":"replace","path":"/{CHUNKS}/Indexes/1/KindDescription/Dimensions","value":{dimensions}}},
            {{"op":"replace","path":"/{CHUNKS}/VectorEmbeddings/0/Model","value":""#
    );
    PatchCollection {
        patch: Text::concat([
            Text::from(prefix),
            Text::reference(MODEL_CAPTURE),
            Text::from(format!(r#"{suffix}"}}]"#)),
        ]),
        expected_error: expected_error.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.switch")]
fn switch_replaces_searchable_chunks() {
    let mut actions = initial();
    actions.extend([
        Request {
            request: format!("query {{ {CHUNKS} {{ _docID }} }}").into(),
            common: Common {
                capture: captures([("oldChunk", format!("/data/{CHUNKS}/0/_docID").as_str())]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        switch(2, "-replacement", None).into(),
        Request {
            request: Text::concat([
                Text::from(format!("query {{ {CHUNKS}(docID: [\"")),
                Text::reference("oldChunk"),
                Text::from("\"]) { text } }"),
            ]),
            expected_data: json!({ CHUNKS: [] }),
            ..Default::default()
        }
        .into(),
        add_article_on(&body(1, "music"), "newArticle", None),
        Request {
            request: format!(
                "query {{ {CHUNKS}(order: {{_alias: {{sim: DESC}}}}, limit: 4) {{
                    text body_v source {{ title }} sim: SIMILARITY(body_v: {{vector: [0, 1]}})
                }} }}"
            )
            .into(),
            expected_data: json!({ CHUNKS: [
                { "text": chunks_of(1, "music")[0], "body_v": [0.0, 1.0], "source": { "title": "t" }, "sim": 1.0 },
                { "text": chunks_of(1, "rivers")[0], "body_v": [0.0, 1.0], "source": { "title": "t" }, "sim": 1.0 },
            ] }),
            common: Common {
                unordered: vec![format!("/data/{CHUNKS}")],
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.switch")]
fn wrong_dimensions_keep_the_old_generation() {
    let mut actions = initial();
    actions.extend([
        switch(4, "-replacement", Some("returned 2 dimensions, expected 4")).into(),
        chunks_have_vectors(&chunks_of(1, "rivers"), None),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.switch")]
fn provider_failure_keeps_the_old_generation() {
    let mut actions = initial();
    actions.extend([
        model_fails().into(),
        switch(2, "", Some(MODEL_FAILED)).into(),
        chunks_have_vectors(&chunks_of(1, "rivers"), None),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.switch")]
fn a_protected_source_requires_bulk_read_authority() {
    let mut patch = switch(2, "-replacement", Some("Permission: bypass-dac"));
    patch.common.identity_seed = Some(BOB);
    TestCase::from_actions(vec![
        model_embedded(&[]),
        AddDACPolicy {
            policy: USER_ACP_POLICY.into(),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: embedded_schema(Some("policy")),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "secret", "body": body(1, "rivers") })),
            common: Common {
                identity_seed: Some(ALICE),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        patch.into(),
    ])
    .execute();
}
