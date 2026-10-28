//! Generated from the conformance scenarios of tests/integration/collection/truncate/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Truncate};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection.truncate.basic")]
fn truncate_collection_truncate_twice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.basic")]
fn truncate_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
