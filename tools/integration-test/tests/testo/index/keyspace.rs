//! A tagged index keyspace keeps collections with colliding short IDs apart.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.keyspace")]
fn a_tagged_index_keyspace_keeps_collections_separate() {
    let mut actions = (1..=118)
        .map(|number| {
            AddCollection {
                sdl: format!("type Tally{number} {{ count: Int @index }}").into(),
                ..Default::default()
            }
            .into()
        })
        .collect::<Vec<_>>();
    for number in [100, 118] {
        actions.push(
            AddDoc {
                collection: format!("Tally{number}"),
                doc_map: Some(json!({ "count": number })),
                ..Default::default()
            }
            .into(),
        );
    }
    for number in [100, 118] {
        actions.push(
            Request {
                request: format!(
                    "query {{ Tally{number}(filter: {{count: {{_eq: {number}}}}}) {{ count }} }}"
                )
                .into(),
                expected_data: json!({ format!("Tally{number}"): [{ "count": number }] }),
                ..Default::default()
            }
            .into(),
        );
    }

    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
