//! Generated from the conformance scenarios of tests/integration/acp/nac/relation_admin/set_migration_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddNACActorRelationship, ConfigureMigration, NodeStopForRestart, Start,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.schema-tools")]
fn nac_admin_relation_can_set_migration() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "John" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                source_version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
                    .into(),
                destination_version_id:
                    "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smv".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: set-migration".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "John" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                source_version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
                    .into(),
                destination_version_id:
                    "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smv".into(),
                common: Common {
                    identity_seed: Some(2),
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
