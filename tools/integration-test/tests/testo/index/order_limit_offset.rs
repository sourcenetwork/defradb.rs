//! An index that only orders applies a page's offset once (#2964).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.order.single-field")]
fn an_ordered_page_skips_its_offset_once() {
    let mut actions = vec![AddCollection {
        sdl: "type T { s: Int @index  u: Int }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend((0..10).map(|n| {
        AddDoc {
            collection: "T".into(),
            doc_map: Some(json!({ "s": n, "u": n })),
            ..Default::default()
        }
        .into()
    }));
    actions.push(
        Request {
            request: "query { T(order: {s: ASC}, limit: 3, offset: 2) { s } }".into(),
            expected_data: json!({ "T": [{ "s": 2 }, { "s": 3 }, { "s": 4 }] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
