//! In a collection with a policy, `_documentArrivals` hands a reader only the
//! rows of documents it may read, and moves its cursor past the others
//! (`defra.events.arrivals.visibility`).

use integration_test::actions::{Actions, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use serde_json::json;

use super::visibility::{as_identity, case, collection, delete, OWNER, STRANGER};

/// `seed`, or nobody, adds a document named `name`, captured as `capture`.
fn add(name: &str, capture: &str, seed: u64) -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..as_identity(seed)
        },
        ..Default::default()
    }
    .into()
}

/// `_documentArrivals` of Users with `args`, as `seed` or as nobody.
fn arrivals(args: &str, seed: Option<u64>, expected: serde_json::Value) -> Actions {
    Request {
        request: Text::from(format!(
            r#"query {{ _documentArrivals(collection: "Users", {args}) {{ head next entries {{ cursor docID }} }} }}"#
        )),
        expected_data: json!({ "_documentArrivals": expected }),
        common: Common {
            identity_seed: seed,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn page(head: &str, next: &str, entries: serde_json::Value) -> serde_json::Value {
    json!({ "head": head, "next": next, "entries": entries })
}

#[test]
#[mantra_macros::req_test("defra.events.arrivals.visibility")]
fn hidden_arrivals_are_passed_and_not_shown() {
    let ann = || json!({ "cursor": "1", "docID": m::r("ann") });
    let bob = || json!({ "cursor": "2", "docID": m::r("bob") });
    let mut actions = collection(true);
    actions.push(add("Ann", "ann", OWNER));
    actions.push(add("Bob", "bob", STRANGER));
    actions.extend([
        arrivals(r#"after: "0""#, Some(OWNER), page("2", "2", json!([ann()]))),
        arrivals(
            r#"after: "0""#,
            Some(STRANGER),
            page("2", "2", json!([bob()])),
        ),
        arrivals(r#"after: "0""#, None, page("2", "2", json!([]))),
        arrivals(
            r#"after: "0", limit: 1"#,
            Some(STRANGER),
            page("2", "1", json!([])),
        ),
        arrivals(
            r#"after: "1", limit: 1"#,
            Some(STRANGER),
            page("2", "2", json!([bob()])),
        ),
        arrivals(
            r#"after: "1", limit: 1"#,
            Some(OWNER),
            page("2", "2", json!([])),
        ),
    ]);
    actions.push(delete("ann"));
    actions.extend([
        arrivals(
            r#"after: "0""#,
            Some(STRANGER),
            page("2", "2", json!([bob()])),
        ),
        arrivals(r#"after: "0""#, None, page("2", "2", json!([]))),
        arrivals(r#"after: "0""#, Some(OWNER), page("2", "2", json!([]))),
    ]);
    let mut case = case(actions);
    case.id = integration_test::scenario_id!();
    case.execute();
}
