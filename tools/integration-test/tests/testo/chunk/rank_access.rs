//! A reader ranks only the articles they may read, through a grant, a
//! revoke and a delete (`defra.chunk.rank.access`).

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, DeleteDACActorRelationship,
    DeleteDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::{json, Value};

use super::access_support::{ALICE, BOB};
use super::embed_support::{embedded_schema, model_embedded};
use super::rank_support::{add_article, add_article_about, body_about, ranked_as, score};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.access")]
fn a_reader_ranks_only_the_articles_they_may_read_through_a_grant_and_a_delete() {
    let mut actions = alice_secret_and_public();
    actions.extend([
        ranked_as(
            ", limit: 2",
            json!([
                { "title": "secret", "sim": score([1.0, 0.0, 0.0]) },
                { "title": "public", "sim": score([0.0, 1.0, 0.0]) },
            ]),
            Some(ALICE),
        )
        .into(),
        // Alice's article is nearer, so a page of one must look past it.
        ranked_as(", limit: 1", public(), Some(BOB)).into(),
        ranked_as(", limit: 1", public(), None).into(),
        grant_bob(),
        ranked_as(", limit: 1", secret(), Some(BOB)).into(),
        ranked_as(", limit: 1", public(), None).into(),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("secret"),
            common: as_alice(),
            ..Default::default()
        }
        .into(),
        ranked_as(", limit: 1", public(), Some(BOB)).into(),
        ranked_as(", limit: 1", public(), Some(ALICE)).into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.access")]
fn a_revoked_reader_stops_ranking_the_article() {
    let mut actions = alice_secret_and_public();
    actions.extend([
        grant_bob(),
        ranked_as(", limit: 1", secret(), Some(BOB)).into(),
        DeleteDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("secret"),
            relation: "reader".into(),
            target_identity_seed: Some(BOB),
            expected_data: json!({ "recordFound": true }),
            common: as_alice(),
            ..Default::default()
        }
        .into(),
        ranked_as(", limit: 1", public(), Some(BOB)).into(),
        ranked_as(", limit: 1", secret(), Some(ALICE)).into(),
    ]);
    TestCase::from_actions(actions).execute();
}

/// Alice's article about rivers under a local policy, and a public one about engines.
fn alice_secret_and_public() -> Vec<Actions> {
    vec![
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
        AddCollection {
            sdl: embedded_schema(Some("policy")),
            ..Default::default()
        }
        .into(),
        add_article(
            json!({ "title": "secret", "body": body_about(&["rivers"]) }),
            Some(ALICE),
            Some("secret"),
        ),
        // Created with no identity, so anyone may read it.
        add_article_about("public", &["engines"], None),
    ]
}

fn grant_bob() -> Actions {
    AddDACActorRelationship {
        collection: "Article".into(),
        doc_id: Text::reference("secret"),
        relation: "reader".into(),
        target_identity_seed: Some(BOB),
        expected_data: json!({ "existedAlready": false }),
        common: as_alice(),
        ..Default::default()
    }
    .into()
}

fn public() -> Value {
    json!([{ "title": "public", "sim": score([0.0, 1.0, 0.0]) }])
}

fn secret() -> Value {
    json!([{ "title": "secret", "sim": score([1.0, 0.0, 0.0]) }])
}

fn as_alice() -> Common {
    Common {
        identity_seed: Some(ALICE),
        ..Default::default()
    }
}
