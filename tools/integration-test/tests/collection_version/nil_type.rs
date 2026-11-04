//! Generated from the conformance scenarios of tests/integration/collection_version/nil_type_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.define.errors")]
fn collection_version_with_missing_type_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User {
                        name:
                    }
                "#
            .into(),
            expected_error: Some("field type not specified. Object: User, Field: name".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
