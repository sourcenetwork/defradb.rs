//! A grant on the parent shows its chunks, and a delete hides them (`defra.chunk.access.grant`).

use integration_test::actions::{AddDACActorRelationship, DeleteDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::access_support::{alice_owns_an_article, read, with_chunk_id, ALICE, BOB, PARAGRAPHS};
use super::support::{as_identity, chunks_are_listed, no_chunks, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.access.grant")]
fn a_reader_of_the_parent_sees_its_chunks_until_it_is_deleted() {
    let commits = with_chunk_id(r#"query { _commits(docID: ""#, r#"") { cid } }"#);
    let mut actions = alice_owns_an_article();
    actions.extend([
        as_identity(no_chunks(None), BOB),
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
        AddDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            relation: "reader".into(),
            target_identity_seed: Some(BOB),
            expected_data: json!({ "existedAlready": false }),
            common: Common {
                identity_seed: Some(ALICE),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        as_identity(chunks_are_listed("plans", PARAGRAPHS, None), BOB),
        read(
            commits.clone(),
            json!({ "_commits": m::nonempty() }),
            Some(BOB),
        ),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            common: Common {
                identity_seed: Some(ALICE),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        as_identity(no_chunks(None), ALICE),
        read(commits, json!({ "_commits": [] }), Some(ALICE)),
    ]);
    TestCase::from_actions(actions).execute();
}
