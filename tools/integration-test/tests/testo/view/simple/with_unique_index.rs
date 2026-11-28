//! Lensed views whose transform gives two items the same value under a unique index.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, AddLens, AddView, RefreshViews, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn clash(id: &'static str, materialized: bool) {
    let directive = if materialized {
        ""
    } else {
        "@materialized(if: false)"
    };
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "age", "value": 23 }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                sdl: format!(
                    r#"
                    type UserView {directive} {{
                        name: String
                        age: Int @index(unique: true)
                    }}
                "#
                )
                .into(),
                transform_cid: Some(Text::reference("lens0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            RefreshViews::default().into(),
            Request {
                request: r#"
                    query {
                        UserView(filter: {age: {_eq: 23}}) {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "UserView": [
                        { "age": 23, "name": "John" },
                        { "age": 23, "name": "Fred" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/UserView".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.view.unique_clash")]
fn view_materialized_transform_giving_two_items_one_unique_value() {
    clash(integration_test::scenario_id!(), true);
}

#[test]
#[mantra_macros::req_test("defra.view.unique_clash")]
fn view_transform_giving_two_items_one_unique_value() {
    clash(integration_test::scenario_id!(), false);
}
