//! A reader ranks only the articles they may read, through a grant and a
//! delete (`defra.chunk.rank.access`).

use integration_test::actions::{AddCollection, AddDACActorRelationship, AddDACPolicy, DeleteDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::json;

use super::access_support::{ALICE, BOB};
use super::embed_support::{embedded_schema, model_embedded};
use super::rank_support::{add_article, add_article_about, body_about, ranked_as, score};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.access")]
fn a_reader_ranks_only_the_articles_they_may_read_through_a_grant_and_a_delete() {
    let public = json!([{ "title": "public", "sim": score([0.0, 1.0, 0.0]) }]);
    let secret = json!([{ "title": "secret", "sim": score([1.0, 0.0, 0.0]) }]);
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
        ranked_as(", limit: 1", public.clone(), Some(BOB)).into(),
        ranked_as(", limit: 1", public.clone(), None).into(),
        AddDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("secret"),
            relation: "reader".into(),
            target_identity_seed: Some(BOB),
            expected_data: json!({ "existedAlready": false }),
            common: as_alice(),
            ..Default::default()
        }
        .into(),
        ranked_as(", limit: 1", secret.clone(), Some(BOB)).into(),
        ranked_as(", limit: 1", public.clone(), None).into(),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("secret"),
            common: as_alice(),
            ..Default::default()
        }
        .into(),
        ranked_as(", limit: 1", public.clone(), Some(BOB)).into(),
        ranked_as(", limit: 1", public, Some(ALICE)).into(),
    ])
    .execute();
}

fn as_alice() -> Common {
    Common {
        identity_seed: Some(ALICE),
        ..Default::default()
    }
}
