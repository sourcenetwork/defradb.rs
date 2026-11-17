//! Each way a request can reach a related document Bob may not read. Alice
//! owns them all, so her answer shows the request does match.

use integration_test::actions::Actions;
use integration_test::m;
use serde_json::json;
use testo::action::ActionInstance;

use super::support::{query, ALICE, BOB};

type Queries = Vec<ActionInstance<Actions>>;

pub fn many_to_one_eq() -> Queries {
    let request = r#"query { Note(filter: {s: {pin: {_eq: "4721"}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn many_to_one_like() -> Queries {
    let request = r#"query { Note(filter: {s: {pin: {_like: "47%"}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn many_to_one_selected() -> Queries {
    let request = r#"query { Note(filter: {s: {pin: {_eq: "4721"}}}) { name s { pin } } }"#;
    vec![
        query(
            ALICE,
            request,
            json!({ "Note": [{ "name": "n1", "s": { "pin": "4721" } }] }),
        ),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn many_to_one_or() -> Queries {
    let request = r#"query {
        Note(filter: {_or: [{s: {pin: {_eq: "4721"}}}, {name: {_eq: "none"}}]}) { name }
    }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn many_to_one_not() -> Queries {
    let request = r#"query { Note(filter: {_not: {s: {pin: {_eq: "4721"}}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n2" }] })),
        query(
            BOB,
            request,
            json!({ "Note": m::unordered(json!([{ "name": "n1" }, { "name": "n2" }])) }),
        ),
    ]
}

pub fn one_to_one_secondary() -> Queries {
    let request = r#"query { Note(filter: {vault: {code: {_eq: "v4721"}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn one_to_many() -> Queries {
    let request = r#"query { Note(filter: {tags: {code: {_eq: "c4721"}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn one_to_many_selected() -> Queries {
    let request =
        r#"query { Note(filter: {tags: {code: {_eq: "c4721"}}}) { name tags { code } } }"#;
    vec![
        query(
            ALICE,
            request,
            json!({ "Note": [{ "name": "n1", "tags": [{ "code": "c4721" }] }] }),
        ),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn multi_level() -> Queries {
    let request = r#"query { Note(filter: {s: {notes: {name: {_eq: "n1"}}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn count() -> Queries {
    let request = r#"query { COUNT(Note: {filter: {s: {pin: {_eq: "4721"}}}}) }"#;
    vec![
        query(ALICE, request, json!({ "COUNT": 1 })),
        query(BOB, request, json!({ "COUNT": 0 })),
    ]
}

pub fn nested_count() -> Queries {
    let request = r#"query {
        Note(filter: {name: {_eq: "n1"}}) { name COUNT(tags: {filter: {code: {_eq: "c4721"}}}) }
    }"#;
    vec![
        query(
            ALICE,
            request,
            json!({ "Note": [{ "name": "n1", "COUNT": 1 }] }),
        ),
        query(
            BOB,
            request,
            json!({ "Note": [{ "name": "n1", "COUNT": 0 }] }),
        ),
    ]
}

pub fn group_by() -> Queries {
    let request = r#"query { Note(groupBy: [name], filter: {s: {pin: {_eq: "4721"}}}) { name } }"#;
    vec![
        query(ALICE, request, json!({ "Note": [{ "name": "n1" }] })),
        query(BOB, request, json!({ "Note": [] })),
    ]
}

pub fn secondary_id() -> Queries {
    let request = r#"query { Note(filter: {name: {_eq: "n1"}}) { name _vaultID } }"#;
    vec![
        query(
            ALICE,
            request,
            json!({ "Note": [{ "name": "n1", "_vaultID": m::r("v1") }] }),
        ),
        query(
            BOB,
            request,
            json!({ "Note": [{ "name": "n1", "_vaultID": null }] }),
        ),
    ]
}

/// n1's pin sorts after n2's. Bob sees neither, so both directions must give
/// him the same order.
pub fn order() -> Queries {
    let asc = r#"query { Note(order: {s: {pin: ASC}}) { name } }"#;
    let desc = r#"query { Note(order: {s: {pin: DESC}}) { name } }"#;
    let names =
        |first: &str, second: &str| json!({ "Note": [{ "name": first }, { "name": second }] });
    vec![
        query(ALICE, asc, names("n2", "n1")),
        query(ALICE, desc, names("n1", "n2")),
        query(BOB, asc, names("n1", "n2")),
        query(BOB, desc, names("n1", "n2")),
    ]
}

/// An order driven by the index on the pin skips notes with no related
/// secret, so Bob gets what he would get if neither note had one.
pub fn order_indexed() -> Queries {
    let asc = r#"query { Note(order: {s: {pin: ASC}}) { name } }"#;
    let desc = r#"query { Note(order: {s: {pin: DESC}}) { name } }"#;
    let names =
        |first: &str, second: &str| json!({ "Note": [{ "name": first }, { "name": second }] });
    vec![
        query(ALICE, asc, names("n2", "n1")),
        query(ALICE, desc, names("n1", "n2")),
        query(BOB, asc, json!({ "Note": [] })),
        query(BOB, desc, json!({ "Note": [] })),
    ]
}
