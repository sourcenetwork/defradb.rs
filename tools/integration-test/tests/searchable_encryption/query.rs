//! Generated from the conformance scenarios of tests/integration/searchable_encryption/query_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, NewNode, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.query.schema")]
fn encrypted_index_new_if_p2_p_is_disabled_can_not_do_se_query() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @encryptedIndex
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "encrypted_User" on type "Query"."#.into(),
                ),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
