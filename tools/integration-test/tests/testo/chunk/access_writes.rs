//! Only a writer of the parent changes its chunks, and a public parent's chunks change for anyone (`defra.chunk.access.writes`, `defra.chunk.access.public`).

use integration_test::actions::{
    Actions, AddDACActorRelationship, AddDoc, DeleteDACActorRelationship, DeleteDoc, UpdateDoc,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::access_support::{alice_owns_an_article, ALICE, BOB, PARAGRAPHS};
use super::support::{as_identity, body, chunk_texts_are, chunks_are_listed, no_chunks};

const REFUSED: &str = "document not found or not authorized to access";

fn as_actor(identity: Option<u64>) -> Common {
    Common {
        identity_seed: identity,
        ..Default::default()
    }
}

fn update(doc: &str, text: &str, identity: Option<u64>, refused: bool) -> Actions {
    UpdateDoc {
        collection: "Article".into(),
        doc_id: Text::reference(doc),
        doc_map: json!({ "body": text }),
        expected_error: refused.then(|| REFUSED.into()),
        common: as_actor(identity),
        ..Default::default()
    }
    .into()
}

fn delete(doc: &str, identity: Option<u64>, refused: bool) -> Actions {
    DeleteDoc {
        collection: "Article".into(),
        doc_id: Text::reference(doc),
        expected_error: refused.then(|| REFUSED.into()),
        common: as_actor(identity),
        ..Default::default()
    }
    .into()
}

fn bob_as_writer(add: bool) -> Actions {
    match add {
        true => AddDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            relation: "writer".into(),
            target_identity_seed: Some(BOB),
            expected_data: json!({ "existedAlready": false }),
            common: as_actor(Some(ALICE)),
            ..Default::default()
        }
        .into(),
        false => DeleteDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            relation: "writer".into(),
            target_identity_seed: Some(BOB),
            expected_data: json!({ "recordFound": true }),
            common: as_actor(Some(ALICE)),
            ..Default::default()
        }
        .into(),
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.access.writes")]
fn only_a_writer_of_the_parent_changes_its_chunks() {
    let (hills, caves) = (body(3, "hills"), body(2, "caves"));
    let mut actions = alice_owns_an_article();
    actions.extend([
        update("article", &hills, Some(BOB), true),
        delete("article", Some(BOB), true),
        update("article", &hills, None, true),
        as_identity(chunks_are_listed("plans", PARAGRAPHS, None), ALICE),
        bob_as_writer(true),
        update("article", &hills, Some(BOB), false),
        as_identity(chunk_texts_are(&[&hills], None), ALICE),
        as_identity(chunk_texts_are(&[&hills], None), BOB),
        no_chunks(None),
        bob_as_writer(false),
        update("article", &caves, Some(BOB), true),
        delete("article", Some(BOB), true),
        as_identity(chunk_texts_are(&[&hills], None), ALICE),
        as_identity(no_chunks(None), BOB),
        delete("article", Some(ALICE), false),
        as_identity(no_chunks(None), ALICE),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.access.public")]
fn anyone_changes_the_chunks_of_a_public_parent() {
    let (rivers, hills) = (body(4, "rivers"), body(3, "hills"));
    let mut actions = alice_owns_an_article();
    actions.extend([
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "open", "body": rivers })),
            common: Common {
                capture: captures([("public", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&rivers], None),
        as_identity(chunk_texts_are(&[&rivers], None), BOB),
        update("public", &hills, Some(BOB), false),
        chunk_texts_are(&[&hills], None),
        as_identity(chunk_texts_are(&[&hills], None), BOB),
        delete("public", None, false),
        no_chunks(None),
        as_identity(no_chunks(None), BOB),
        as_identity(chunks_are_listed("plans", PARAGRAPHS, None), ALICE),
    ]);
    TestCase::from_actions(actions).execute();
}
