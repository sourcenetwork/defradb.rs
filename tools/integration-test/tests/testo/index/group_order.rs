//! Groups follow the select's order over an index that orders it, even when
//! `_group` has an order of its own.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.order")]
fn a_group_order_keeps_the_indexed_group_order() {
    let mut actions = vec![AddCollection {
        sdl: "type Users { Name: String @index  Age: Int }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend(
        [("John", 25), ("John", 32), ("Carlo", 55), ("Alice", 19)].map(|(name, age)| {
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": name, "Age": age })),
                ..Default::default()
            }
            .into()
        }),
    );
    actions.push(
        Request {
            request: "query { Users(groupBy: [Name], order: {Name: DESC}) { \
                      Name GROUP(order: {Age: ASC}) { Age } } }"
                .into(),
            expected_data: json!({ "Users": [
                { "Name": "John", "GROUP": [{ "Age": 25 }, { "Age": 32 }] },
                { "Name": "Carlo", "GROUP": [{ "Age": 55 }] },
                { "Name": "Alice", "GROUP": [{ "Age": 19 }] },
            ] }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
