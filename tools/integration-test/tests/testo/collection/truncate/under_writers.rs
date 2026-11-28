use integration_test::actions::{AddCollection, Parallel, Request, Truncate};
use integration_test::scenario::{Common, Eventually};
use integration_test::test_case::TestCase;
use serde_json::json;
const DOCS: usize = 200;
const WRITERS: usize = 8;

/// An update of every document of group `g` that keeps running until the
/// truncate has taken them all.
fn writer(g: usize) -> Request {
    Request {
        request: format!(
            "mutation {{ update_U(filter: {{g: {{_eq: {g}}}}}, input: {{n: 1}}) {{ _docID }} }}"
        )
        .into(),
        expected_data: json!({ "update_U": [] }),
        common: Common {
            eventually: Some(Eventually {
                timeout_millis: Some(120_000),
                interval_millis: Some(1),
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate_under_writers")]
fn truncate_finishes_while_writers_update_the_collection() {
    case("type U { n: Int @crdt(type: pcounter) @index g: Int i: Int }");
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate_under_writers")]
fn truncate_finishes_while_writers_update_an_unindexed_field() {
    case("type U { n: Int @crdt(type: pcounter) g: Int i: Int }");
}

fn case(sdl: &str) {
    let docs = (0..DOCS)
        .map(|i| format!("{{n: 0, g: {}, i: {i}}}", i % WRITERS))
        .collect::<Vec<_>>()
        .join(", ");
    let mut actions = vec![Truncate {
        collection: "U".into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..WRITERS).map(|g| writer(g).into()));
    TestCase::from_actions(vec![
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("mutation {{ add_U(input: [{docs}]) {{ _docID }} }}").into(),
            ..Default::default()
        }
        .into(),
        Parallel {
            actions,
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { U(showDeleted: true) { _docID } }".into(),
            expected_data: json!({ "U": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
