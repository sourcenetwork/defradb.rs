//! Generated from the conformance scenarios of tests/integration/index/async_backfill_migration_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec, Lens, LensModule};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, Await, IndexReleaseBuildGate, IndexWaitBuildBlocked,
    IndexWaitBuildCollections, ListIndexes, NewIndex, NodeOpenIndexGatedPersistent,
    NodeOpenIndexMultiGatedPersistent, NodeOpenIndexWatermarkGatedPersistent, PatchCollection,
    Request, Restart, WaitForIndexReady,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

#[test]
#[mantra_macros::req_test("go.index.backfill.migration")]
fn async_backfill_epoch_advanced_mid_build_live_epoch_complete() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexGatedPersistent {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        level: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n000", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n001", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n002", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n003", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n004", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n005", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n006", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n007", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n008", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n009", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n010", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n011", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n012", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n013", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n014", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n015", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n016", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n017", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n018", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n019", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n020", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n021", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n022", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n023", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n024", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n025", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n026", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n027", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n028", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n029", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n030", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n031", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n032", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n033", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n034", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n035", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n036", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n037", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n038", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n039", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n040", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n041", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n042", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n043", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n044", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n045", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n046", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n047", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n048", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n049", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n050", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n051", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n052", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n053", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n054", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n055", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n056", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n057", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n058", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n059", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n060", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n061", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n062", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n063", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n064", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n065", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n066", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n067", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n068", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n069", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n070", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n071", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n072", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n073", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n074", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n075", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n076", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n077", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n078", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n079", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n080", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n081", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n082", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n083", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n084", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n085", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n086", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n087", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n088", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n089", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n090", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n091", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n092", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n093", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n094", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n095", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n096", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n097", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n098", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n099", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n100", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n101", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n102", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n103", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n104", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n105", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n106", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n107", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n108", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n109", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n110", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n111", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n112", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n113", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n114", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n115", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n116", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n117", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n118", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n119", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n120", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n121", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n122", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n123", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n124", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n125", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n126", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n127", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n128", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n129", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n130", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n131", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n132", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n133", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n134", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n135", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n136", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n137", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n138", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n139", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n140", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n141", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n142", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n143", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n144", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n145", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n146", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n147", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n148", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "n149", "level": 1 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                r#async: true,
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"add","path":"/User/Fields/-","value":{"Name":"score","Kind":"Int"}}]"#.into(),
                lens: Some(Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/increment.wasm".into(),
                            arguments: json!({ "field": "level", "value": 1 }),
                        },
                    ]),
                }),
                r#async: true,
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            Await {
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: "User".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query @explain(type: execute)  { User(filter: {name: {_eq: "n000"}}) { level } }"#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { User(filter: {name: {_eq: "n000"}}) { level } }"#.into(),
                expected_data: json!({ "User": [{ "level": 2 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User(order: {name: ASC}) { name } }".into(),
                expected_data: json!({
                    "User": [
                        { "name": "n000" },
                        { "name": "n001" },
                        { "name": "n002" },
                        { "name": "n003" },
                        { "name": "n004" },
                        { "name": "n005" },
                        { "name": "n006" },
                        { "name": "n007" },
                        { "name": "n008" },
                        { "name": "n009" },
                        { "name": "n010" },
                        { "name": "n011" },
                        { "name": "n012" },
                        { "name": "n013" },
                        { "name": "n014" },
                        { "name": "n015" },
                        { "name": "n016" },
                        { "name": "n017" },
                        { "name": "n018" },
                        { "name": "n019" },
                        { "name": "n020" },
                        { "name": "n021" },
                        { "name": "n022" },
                        { "name": "n023" },
                        { "name": "n024" },
                        { "name": "n025" },
                        { "name": "n026" },
                        { "name": "n027" },
                        { "name": "n028" },
                        { "name": "n029" },
                        { "name": "n030" },
                        { "name": "n031" },
                        { "name": "n032" },
                        { "name": "n033" },
                        { "name": "n034" },
                        { "name": "n035" },
                        { "name": "n036" },
                        { "name": "n037" },
                        { "name": "n038" },
                        { "name": "n039" },
                        { "name": "n040" },
                        { "name": "n041" },
                        { "name": "n042" },
                        { "name": "n043" },
                        { "name": "n044" },
                        { "name": "n045" },
                        { "name": "n046" },
                        { "name": "n047" },
                        { "name": "n048" },
                        { "name": "n049" },
                        { "name": "n050" },
                        { "name": "n051" },
                        { "name": "n052" },
                        { "name": "n053" },
                        { "name": "n054" },
                        { "name": "n055" },
                        { "name": "n056" },
                        { "name": "n057" },
                        { "name": "n058" },
                        { "name": "n059" },
                        { "name": "n060" },
                        { "name": "n061" },
                        { "name": "n062" },
                        { "name": "n063" },
                        { "name": "n064" },
                        { "name": "n065" },
                        { "name": "n066" },
                        { "name": "n067" },
                        { "name": "n068" },
                        { "name": "n069" },
                        { "name": "n070" },
                        { "name": "n071" },
                        { "name": "n072" },
                        { "name": "n073" },
                        { "name": "n074" },
                        { "name": "n075" },
                        { "name": "n076" },
                        { "name": "n077" },
                        { "name": "n078" },
                        { "name": "n079" },
                        { "name": "n080" },
                        { "name": "n081" },
                        { "name": "n082" },
                        { "name": "n083" },
                        { "name": "n084" },
                        { "name": "n085" },
                        { "name": "n086" },
                        { "name": "n087" },
                        { "name": "n088" },
                        { "name": "n089" },
                        { "name": "n090" },
                        { "name": "n091" },
                        { "name": "n092" },
                        { "name": "n093" },
                        { "name": "n094" },
                        { "name": "n095" },
                        { "name": "n096" },
                        { "name": "n097" },
                        { "name": "n098" },
                        { "name": "n099" },
                        { "name": "n100" },
                        { "name": "n101" },
                        { "name": "n102" },
                        { "name": "n103" },
                        { "name": "n104" },
                        { "name": "n105" },
                        { "name": "n106" },
                        { "name": "n107" },
                        { "name": "n108" },
                        { "name": "n109" },
                        { "name": "n110" },
                        { "name": "n111" },
                        { "name": "n112" },
                        { "name": "n113" },
                        { "name": "n114" },
                        { "name": "n115" },
                        { "name": "n116" },
                        { "name": "n117" },
                        { "name": "n118" },
                        { "name": "n119" },
                        { "name": "n120" },
                        { "name": "n121" },
                        { "name": "n122" },
                        { "name": "n123" },
                        { "name": "n124" },
                        { "name": "n125" },
                        { "name": "n126" },
                        { "name": "n127" },
                        { "name": "n128" },
                        { "name": "n129" },
                        { "name": "n130" },
                        { "name": "n131" },
                        { "name": "n132" },
                        { "name": "n133" },
                        { "name": "n134" },
                        { "name": "n135" },
                        { "name": "n136" },
                        { "name": "n137" },
                        { "name": "n138" },
                        { "name": "n139" },
                        { "name": "n140" },
                        { "name": "n141" },
                        { "name": "n142" },
                        { "name": "n143" },
                        { "name": "n144" },
                        { "name": "n145" },
                        { "name": "n146" },
                        { "name": "n147" },
                        { "name": "n148" },
                        { "name": "n149" },
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

fn async_backfill_restart_after_non_zero_watermark_resumes_to_ready_actions_0(
) -> Vec<ActionInstance<Actions>> {
    vec![
        NodeOpenIndexWatermarkGatedPersistent {
            ..Default::default()
        }
        .into(),
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
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n000" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n001" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n002" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n003" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n004" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n005" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n006" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n007" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n008" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n009" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n010" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n011" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n012" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n013" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n014" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n015" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n016" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n017" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n018" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n019" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n020" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n021" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n022" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n023" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n024" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n025" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n026" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n027" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n028" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n029" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n030" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n031" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n032" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n033" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n034" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n035" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n036" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n037" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n038" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n039" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n040" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n041" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n042" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n043" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n044" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n045" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n046" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n047" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n048" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n049" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n050" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n051" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n052" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n053" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n054" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n055" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n056" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n057" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n058" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n059" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n060" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n061" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n062" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n063" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n064" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n065" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n066" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n067" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n068" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n069" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n070" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n071" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n072" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n073" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n074" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n075" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n076" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n077" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n078" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n079" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n080" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n081" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n082" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n083" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n084" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n085" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n086" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n087" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n088" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n089" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n090" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n091" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n092" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n093" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n094" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n095" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n096" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n097" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n098" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n099" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n100" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n101" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n102" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n103" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n104" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n105" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n106" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n107" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n108" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n109" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n110" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n111" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n112" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n113" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n114" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n115" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n116" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n117" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n118" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n119" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n120" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n121" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n122" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n123" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n124" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n125" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n126" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n127" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n128" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n129" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n130" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n131" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n132" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n133" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n134" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n135" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n136" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n137" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n138" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n139" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n140" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n141" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n142" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n143" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n144" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n145" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n146" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n147" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n148" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n149" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n150" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n151" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n152" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n153" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n154" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n155" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n156" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n157" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n158" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n159" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n160" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n161" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n162" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n163" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n164" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n165" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n166" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n167" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n168" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n169" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n170" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n171" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n172" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n173" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n174" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n175" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n176" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n177" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n178" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n179" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n180" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n181" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n182" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n183" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n184" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n185" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n186" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n187" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n188" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n189" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n190" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n191" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n192" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n193" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n194" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n195" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n196" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n197" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n198" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n199" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n200" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n201" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n202" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n203" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n204" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n205" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n206" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n207" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n208" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n209" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n210" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n211" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n212" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n213" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n214" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n215" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n216" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n217" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n218" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n219" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n220" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n221" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n222" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n223" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n224" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n225" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n226" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n227" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n228" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n229" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n230" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n231" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n232" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n233" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n234" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n235" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n236" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n237" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n238" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n239" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n240" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n241" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n242" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n243" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n244" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n245" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n246" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n247" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn async_backfill_restart_after_non_zero_watermark_resumes_to_ready_actions_1(
) -> Vec<ActionInstance<Actions>> {
    vec![
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n248" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "n249" })),
            common: Common {
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        NewIndex {
            collection: "User".into(),
            index: IndexSpec {
                name: "".into(),
                unique: false,
                fields: vec![IndexField {
                    name: "name".into(),
                    descending: false,
                }],
                ..Default::default()
            },
            r#async: true,
            ..Default::default()
        }
        .into(),
        IndexWaitBuildBlocked {
            ..Default::default()
        }
        .into(),
        Restart {
            ..Default::default()
        }
        .into(),
        IndexReleaseBuildGate {
            ..Default::default()
        }
        .into(),
        WaitForIndexReady {
            collection: "User".into(),
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: "User".into(),
            expected_data: json!({
                "indexes": m::unordered(json!([
                    m::object(json!({
                        "Description": m::object(json!({
                            "Name": "User_name_ASC",
                            "ID": 1,
                            "Kind": 0,
                            "Unique": false,
                            "Fields": m::unordered(json!([
                                m::object(json!({ "Name": "name", "Descending": false })),
                            ])),
                        })),
                        "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                    })),
                ])),
            }),
            ..Default::default()
        }
        .into(),
        Request {
            request:
                r#"query @explain(type: execute)  { User(filter: {name: {_eq: "n000"}}) { name } }"#
                    .into(),
            expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { User(order: {name: ASC}) { name } }".into(),
            expected_data: json!({
                "User": [
                    { "name": "n000" },
                    { "name": "n001" },
                    { "name": "n002" },
                    { "name": "n003" },
                    { "name": "n004" },
                    { "name": "n005" },
                    { "name": "n006" },
                    { "name": "n007" },
                    { "name": "n008" },
                    { "name": "n009" },
                    { "name": "n010" },
                    { "name": "n011" },
                    { "name": "n012" },
                    { "name": "n013" },
                    { "name": "n014" },
                    { "name": "n015" },
                    { "name": "n016" },
                    { "name": "n017" },
                    { "name": "n018" },
                    { "name": "n019" },
                    { "name": "n020" },
                    { "name": "n021" },
                    { "name": "n022" },
                    { "name": "n023" },
                    { "name": "n024" },
                    { "name": "n025" },
                    { "name": "n026" },
                    { "name": "n027" },
                    { "name": "n028" },
                    { "name": "n029" },
                    { "name": "n030" },
                    { "name": "n031" },
                    { "name": "n032" },
                    { "name": "n033" },
                    { "name": "n034" },
                    { "name": "n035" },
                    { "name": "n036" },
                    { "name": "n037" },
                    { "name": "n038" },
                    { "name": "n039" },
                    { "name": "n040" },
                    { "name": "n041" },
                    { "name": "n042" },
                    { "name": "n043" },
                    { "name": "n044" },
                    { "name": "n045" },
                    { "name": "n046" },
                    { "name": "n047" },
                    { "name": "n048" },
                    { "name": "n049" },
                    { "name": "n050" },
                    { "name": "n051" },
                    { "name": "n052" },
                    { "name": "n053" },
                    { "name": "n054" },
                    { "name": "n055" },
                    { "name": "n056" },
                    { "name": "n057" },
                    { "name": "n058" },
                    { "name": "n059" },
                    { "name": "n060" },
                    { "name": "n061" },
                    { "name": "n062" },
                    { "name": "n063" },
                    { "name": "n064" },
                    { "name": "n065" },
                    { "name": "n066" },
                    { "name": "n067" },
                    { "name": "n068" },
                    { "name": "n069" },
                    { "name": "n070" },
                    { "name": "n071" },
                    { "name": "n072" },
                    { "name": "n073" },
                    { "name": "n074" },
                    { "name": "n075" },
                    { "name": "n076" },
                    { "name": "n077" },
                    { "name": "n078" },
                    { "name": "n079" },
                    { "name": "n080" },
                    { "name": "n081" },
                    { "name": "n082" },
                    { "name": "n083" },
                    { "name": "n084" },
                    { "name": "n085" },
                    { "name": "n086" },
                    { "name": "n087" },
                    { "name": "n088" },
                    { "name": "n089" },
                    { "name": "n090" },
                    { "name": "n091" },
                    { "name": "n092" },
                    { "name": "n093" },
                    { "name": "n094" },
                    { "name": "n095" },
                    { "name": "n096" },
                    { "name": "n097" },
                    { "name": "n098" },
                    { "name": "n099" },
                    { "name": "n100" },
                    { "name": "n101" },
                    { "name": "n102" },
                    { "name": "n103" },
                    { "name": "n104" },
                    { "name": "n105" },
                    { "name": "n106" },
                    { "name": "n107" },
                    { "name": "n108" },
                    { "name": "n109" },
                    { "name": "n110" },
                    { "name": "n111" },
                    { "name": "n112" },
                    { "name": "n113" },
                    { "name": "n114" },
                    { "name": "n115" },
                    { "name": "n116" },
                    { "name": "n117" },
                    { "name": "n118" },
                    { "name": "n119" },
                    { "name": "n120" },
                    { "name": "n121" },
                    { "name": "n122" },
                    { "name": "n123" },
                    { "name": "n124" },
                    { "name": "n125" },
                    { "name": "n126" },
                    { "name": "n127" },
                    { "name": "n128" },
                    { "name": "n129" },
                    { "name": "n130" },
                    { "name": "n131" },
                    { "name": "n132" },
                    { "name": "n133" },
                    { "name": "n134" },
                    { "name": "n135" },
                    { "name": "n136" },
                    { "name": "n137" },
                    { "name": "n138" },
                    { "name": "n139" },
                    { "name": "n140" },
                    { "name": "n141" },
                    { "name": "n142" },
                    { "name": "n143" },
                    { "name": "n144" },
                    { "name": "n145" },
                    { "name": "n146" },
                    { "name": "n147" },
                    { "name": "n148" },
                    { "name": "n149" },
                    { "name": "n150" },
                    { "name": "n151" },
                    { "name": "n152" },
                    { "name": "n153" },
                    { "name": "n154" },
                    { "name": "n155" },
                    { "name": "n156" },
                    { "name": "n157" },
                    { "name": "n158" },
                    { "name": "n159" },
                    { "name": "n160" },
                    { "name": "n161" },
                    { "name": "n162" },
                    { "name": "n163" },
                    { "name": "n164" },
                    { "name": "n165" },
                    { "name": "n166" },
                    { "name": "n167" },
                    { "name": "n168" },
                    { "name": "n169" },
                    { "name": "n170" },
                    { "name": "n171" },
                    { "name": "n172" },
                    { "name": "n173" },
                    { "name": "n174" },
                    { "name": "n175" },
                    { "name": "n176" },
                    { "name": "n177" },
                    { "name": "n178" },
                    { "name": "n179" },
                    { "name": "n180" },
                    { "name": "n181" },
                    { "name": "n182" },
                    { "name": "n183" },
                    { "name": "n184" },
                    { "name": "n185" },
                    { "name": "n186" },
                    { "name": "n187" },
                    { "name": "n188" },
                    { "name": "n189" },
                    { "name": "n190" },
                    { "name": "n191" },
                    { "name": "n192" },
                    { "name": "n193" },
                    { "name": "n194" },
                    { "name": "n195" },
                    { "name": "n196" },
                    { "name": "n197" },
                    { "name": "n198" },
                    { "name": "n199" },
                    { "name": "n200" },
                    { "name": "n201" },
                    { "name": "n202" },
                    { "name": "n203" },
                    { "name": "n204" },
                    { "name": "n205" },
                    { "name": "n206" },
                    { "name": "n207" },
                    { "name": "n208" },
                    { "name": "n209" },
                    { "name": "n210" },
                    { "name": "n211" },
                    { "name": "n212" },
                    { "name": "n213" },
                    { "name": "n214" },
                    { "name": "n215" },
                    { "name": "n216" },
                    { "name": "n217" },
                    { "name": "n218" },
                    { "name": "n219" },
                    { "name": "n220" },
                    { "name": "n221" },
                    { "name": "n222" },
                    { "name": "n223" },
                    { "name": "n224" },
                    { "name": "n225" },
                    { "name": "n226" },
                    { "name": "n227" },
                    { "name": "n228" },
                    { "name": "n229" },
                    { "name": "n230" },
                    { "name": "n231" },
                    { "name": "n232" },
                    { "name": "n233" },
                    { "name": "n234" },
                    { "name": "n235" },
                    { "name": "n236" },
                    { "name": "n237" },
                    { "name": "n238" },
                    { "name": "n239" },
                    { "name": "n240" },
                    { "name": "n241" },
                    { "name": "n242" },
                    { "name": "n243" },
                    { "name": "n244" },
                    { "name": "n245" },
                    { "name": "n246" },
                    { "name": "n247" },
                    { "name": "n248" },
                    { "name": "n249" },
                ],
            }),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("go.index.backfill.migration")]
fn async_backfill_restart_after_non_zero_watermark_resumes_to_ready() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: [
            async_backfill_restart_after_non_zero_watermark_resumes_to_ready_actions_0(),
            async_backfill_restart_after_non_zero_watermark_resumes_to_ready_actions_1(),
        ]
        .concat(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.backfill.migration")]
fn async_backfill_two_collections_migrated_together_both_reindex_concurrently_and_converge() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexMultiGatedPersistent {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rank: Int @index
                    }
                    type Author {
                        name: String
                        rank: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Dune", "rank": 20 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Herbert", "rank": 10 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
						[
							{ "op": "add", "path": "/Book/Fields/-", "value": {"Name": "pages", "Kind": "Int"} },
							{ "op": "add", "path": "/Author/Fields/-", "value": {"Name": "born", "Kind": "Int"} }
						]
					"#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "field": "rank", "value": 5 }),
                        path: "bin/lenses/increment.wasm".into(),
                    }]),
                }),
                r#async: true,
                ..Default::default()
            }
            .into(),
            IndexWaitBuildCollections {
                expected_count: 2,
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Book_rank_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "rank", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 1, "Action": 3 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Author_rank_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "rank", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 1, "Action": 3 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            Await {
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Book_rank_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "rank", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Author_rank_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "rank", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Book(filter: {rank: {_eq: 25}}) { title } }".into(),
                expected_data: json!({ "Book": [{ "title": "Dune" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request:
                    "query @explain(type: execute)  { Book(filter: {rank: {_eq: 25}}) { title } }"
                        .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Author(filter: {rank: {_eq: 15}}) { name } }".into(),
                expected_data: json!({ "Author": [{ "name": "Herbert" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request:
                    "query @explain(type: execute)  { Author(filter: {rank: {_eq: 15}}) { name } }"
                        .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
