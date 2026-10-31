//! Generated from the conformance scenarios of tests/integration/collection_version/patch_migration_numeric_fidelity_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddLens, ListLenses};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.lens_registry")]
fn collection_version_patch_large_integer_lens_arguments_are_not_deduplicated() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "verified", "value": 9007199254740993_i64 }),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "verified", "value": 9007199254740992_i64 }),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            ListLenses {
                expected_count: json!(m::object(json!({ "count": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
