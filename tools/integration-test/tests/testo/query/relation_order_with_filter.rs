//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.
//!
//! Ordering by a field of a relation while filtering on another field of it
//! (https://github.com/sourcenetwork/defradb.rs/issues/3244). `publisher` is
//! one-to-one with the foreign key on Publisher, `author` is one-to-many with
//! the foreign key on Book. Each test runs without and with indexes on the
//! filtered and ordered fields.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

fn sdl(indexed: bool) -> String {
    let index = if indexed { " @index" } else { "" };
    format!(
        r#"
        type Book {{
            name: String
            publisher: Publisher
            author: Author
        }}
        type Publisher {{
            name: String
            rank: Int{index}
            score: Int{index}
            book: Book @primary
        }}
        type Author {{
            name: String
            rank: Int{index}
            score: Int{index}
            books: [Book]
        }}
        "#
    )
}

fn doc(collection: &str, name: &str, fields: Value) -> Action {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(fields),
        common: Common {
            capture: captures([(name, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Ann (rank 1, score 9), Bob (2, 2) and Cid (3, 5), each with a book, a
/// publisher and an author of the same name.
fn dataset(indexed: bool) -> Vec<Action> {
    let people = [
        ("ann", "Ann", 1, 9),
        ("bob", "Bob", 2, 2),
        ("cid", "Cid", 3, 5),
    ];
    let mut actions: Vec<Action> = vec![AddCollection {
        sdl: Text::Plain(sdl(indexed)),
        ..Default::default()
    }
    .into()];
    for (key, name, rank, score) in people {
        let author = format!("author_{key}");
        let book = format!("book_{key}");
        actions.push(doc(
            "Author",
            &author,
            json!({ "name": name, "rank": rank, "score": score }),
        ));
        actions.push(doc(
            "Book",
            &book,
            json!({ "name": name, "author": m::r(&author) }),
        ));
        actions.push(doc(
            "Publisher",
            &format!("publisher_{key}"),
            json!({ "name": name, "rank": rank, "score": score, "book": m::r(&book) }),
        ));
    }
    actions
}

fn request(query: String, expected: Value) -> Action {
    Request {
        request: Text::Plain(query),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

fn names(names: &[&str]) -> Value {
    json!({ "Book": names.iter().map(|n| json!({ "name": n })).collect::<Vec<_>>() })
}

/// Runs `args` and `selection` against `publisher` and then `author`.
fn run(id: &'static str, indexed: bool, args: &str, selection: &str, expected: Value) {
    let mut actions = dataset(indexed);
    for relation in ["publisher", "author"] {
        let args = args.replace("REL", relation);
        let selection = selection.replace("REL", relation);
        let expected = serde_json::from_str(&expected.to_string().replace("REL", relation))
            .expect("expected data is valid JSON");
        actions.push(request(
            format!("query {{ Book({args}) {{ {selection} }} }}"),
            expected,
        ));
    }
    TestCase {
        id,
        actions,
        ..Default::default()
    }
    .execute();
}

macro_rules! scenario {
    ($(#[$doc:meta])* $name:ident, $args:expr, $selection:expr, $expected:expr) => {
        mod $name {
            use super::*;

            $(#[$doc])*
            #[test]
            #[mantra_macros::req_test("defra.query.relation_order.filter_other_field")]
            fn unindexed() {
                run(integration_test::scenario_id!(), false, $args, $selection, $expected);
            }

            $(#[$doc])*
            #[test]
            #[mantra_macros::req_test("defra.query.relation_order.filter_other_field")]
            fn indexed() {
                run(integration_test::scenario_id!(), true, $args, $selection, $expected);
            }
        }
    };
}

scenario!(
    /// The sort holds when the relation is only filtered and ordered.
    ascending,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: ASC}}",
    "name",
    names(&["Bob", "Cid", "Ann"])
);

scenario!(
    /// Descending order is the reverse.
    descending,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: DESC}}",
    "name",
    names(&["Ann", "Cid", "Bob"])
);

scenario!(
    /// A limit takes the first rows of the sorted result.
    ascending_with_limit,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: ASC}}, limit: 2",
    "name",
    names(&["Bob", "Cid"])
);

scenario!(
    /// The filter still drops the rows it rejects.
    descending_with_narrowing_filter,
    "filter: {REL: {rank: {_gt: 1}}}, order: {REL: {score: DESC}}",
    "name",
    names(&["Cid", "Bob"])
);

scenario!(
    /// Selecting the relation renders only its selected fields.
    selected_relation,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: ASC}}",
    "name REL { score }",
    json!({ "Book": [
        { "name": "Bob", "REL": { "score": 2 } },
        { "name": "Cid", "REL": { "score": 5 } },
        { "name": "Ann", "REL": { "score": 9 } },
    ] })
);

scenario!(
    /// Filtering and ordering on the same field.
    same_field_filter,
    "filter: {REL: {score: {_gt: 0}}}, order: {REL: {score: ASC}}",
    "name",
    names(&["Bob", "Cid", "Ann"])
);

fn names_with_ids(people: &[(&str, &str)]) -> Value {
    json!({ "Book": people
        .iter()
        .map(|(name, key)| json!({ "name": name, "_RELID": m::r(&format!("REL_{key}")) }))
        .collect::<Vec<_>>() })
}

scenario!(
    /// Selecting only the relation ID keeps the sort fields.
    selected_relation_id,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: ASC}}",
    "name _RELID",
    names_with_ids(&[("Bob", "bob"), ("Cid", "cid"), ("Ann", "ann")])
);

scenario!(
    /// The same with a limit.
    selected_relation_id_with_limit,
    "filter: {REL: {rank: {_gt: 0}}}, order: {REL: {score: ASC}}, limit: 2",
    "name _RELID",
    names_with_ids(&[("Bob", "bob"), ("Cid", "cid")])
);

scenario!(
    /// Selecting only the relation ID without a filter.
    selected_relation_id_unfiltered,
    "order: {REL: {score: ASC}}",
    "name _RELID",
    names_with_ids(&[("Bob", "bob"), ("Cid", "cid"), ("Ann", "ann")])
);

scenario!(
    /// The same with a limit.
    selected_relation_id_unfiltered_with_limit,
    "order: {REL: {score: ASC}}, limit: 2",
    "name _RELID",
    names_with_ids(&[("Bob", "bob"), ("Cid", "cid")])
);
