//! An `_alias` condition under `_or` is decided after the scores exist
//! (`defra.query.simple.similarity.alias_under_combinator`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

fn user(name: &str, status: &str, points: [i64; 2]) -> AddDoc {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "status": status, "points": points })),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.query.simple.similarity.alias_under_combinator")]
fn similarity_alias_under_or_keeps_rows_the_alias_matches() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String status: String points: [Int!] }".into(),
                ..Default::default()
            }
            .into(),
            user("Ann", "public", [0, 1]).into(),
            user("Bob", "private", [1, 0]).into(),
            user("Cat", "private", [0, 1]).into(),
            Request {
                request: r#"query {
                    User(
                        filter: {_or: [{status: {_eq: "public"}}, {_alias: {sim: {_gt: 0.9}}}]},
                        order: {name: ASC}
                    ) {
                        name
                        sim: SIMILARITY(points: {vector: [1, 0]})
                    }
                }"#
                .into(),
                expected_data: json!({
                    "User": [
                        { "name": "Ann", "sim": m::similarity_score("COSINE", json!([0, 1]), json!([1, 0])) },
                        { "name": "Bob", "sim": m::similarity_score("COSINE", json!([1, 0]), json!([1, 0])) },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
