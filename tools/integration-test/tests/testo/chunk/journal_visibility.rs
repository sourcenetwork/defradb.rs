//! The change journals of a `@chunk` collection hand a reader only the rows of
//! chunks it may read, and move their cursors past the others
//! (`defra.events.arrivals.chunk`, `defra.events.delivery.chunk`).

use integration_test::actions::{Actions, DeleteDoc, Request};
use integration_test::m;
use integration_test::scenario::{Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

use super::access_support::{alice_owns_an_article, ALICE, BOB, PARAGRAPHS};
use super::support::{add_article, add_article_collection, body, update_article, CHUNKS};

/// `journal` of the chunk collection with `args`, as `identity` or as nobody.
fn journal(journal: &str, args: &str, identity: Option<u64>, expected: Value) -> Actions {
    Request {
        request: Text::from(format!(
            r#"query {{ {journal}(collection: "{CHUNKS}", {args}) {{ head next entries {{ cursor }} }} }}"#
        )),
        expected_data: json!({ journal: expected }),
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn arrivals(args: &str, identity: Option<u64>, expected: Value) -> Actions {
    journal("_documentArrivals", args, identity, expected)
}

fn page(head: usize, next: usize, cursors: std::ops::RangeInclusive<usize>) -> Value {
    let entries: Vec<Value> = cursors
        .map(|cursor| json!({ "cursor": cursor.to_string() }))
        .collect();
    json!({ "head": head.to_string(), "next": next.to_string(), "entries": entries })
}

fn case(actions: Vec<Actions>) -> TestCase {
    let mut case = TestCase::from_actions(actions);
    // `_documentArrivals` and `_documentUpdates` are defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case
}

#[test]
#[mantra_macros::req_test("defra.events.arrivals.chunk")]
fn a_chunks_arrival_is_passed_for_a_reader_who_may_not_read_its_parent() {
    let n = PARAGRAPHS;
    let mut actions = alice_owns_an_article();
    actions.extend([
        arrivals(r#"after: "0""#, Some(ALICE), page(n, n, 1..=n)),
        arrivals(r#"after: "0", limit: 2"#, Some(ALICE), page(n, 2, 1..=2)),
        arrivals(r#"after: "0""#, Some(BOB), page(n, n, 1..=0)),
        arrivals(r#"after: "0", limit: 2"#, Some(BOB), page(n, 2, 1..=0)),
        arrivals(r#"after: "2""#, Some(BOB), page(n, n, 1..=0)),
        arrivals(r#"after: "0""#, None, page(n, n, 1..=0)),
    ]);
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.events.arrivals.chunk")]
fn without_a_policy_only_the_chunks_of_a_live_parents_text_are_shown() {
    let mut actions = vec![
        add_article_collection(),
        add_article(&body(2, "plans"), "article"),
        arrivals(r#"after: "0""#, None, page(2, 2, 1..=2)),
        arrivals(r#"after: "0""#, Some(BOB), page(2, 2, 1..=2)),
        update_article("article", &body(2, "maps")),
        arrivals(r#"after: "0""#, None, page(4, 4, 3..=4)),
        arrivals(r#"after: "0", limit: 2"#, None, page(4, 2, 1..=0)),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            ..Default::default()
        }
        .into(),
    ];
    for identity in [None, Some(ALICE)] {
        actions.push(arrivals(r#"after: "0""#, identity, page(4, 4, 1..=0)));
    }
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.events.delivery.chunk")]
fn a_parents_writes_add_no_row_to_its_chunks_update_journal() {
    let mut actions = alice_owns_an_article();
    actions.push(update_article_as_alice());
    for identity in [Some(ALICE), Some(BOB), None] {
        actions.push(journal(
            "_documentUpdates",
            r#"after: "0""#,
            identity,
            json!({ "head": "0", "next": "0", "entries": [] }),
        ));
    }
    actions.push(Request {
        request: r#"query { _documentUpdates(collection: "Article", after: "0") { head entries { docID } } }"#.into(),
        expected_data: json!({ "_documentUpdates": {
            "head": "1",
            "entries": [{ "docID": m::r("article") }],
        } }),
        common: Common {
            identity_seed: Some(ALICE),
            ..Default::default()
        },
        ..Default::default()
    }
    .into());
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}

fn update_article_as_alice() -> Actions {
    super::support::as_identity(update_article("article", &body(2, "maps")), ALICE)
}
