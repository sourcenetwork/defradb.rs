//! Admin operations cannot change the chunk collection (`defra.chunk.protect.admin_ops`).

use integration_test::actions::shared::{IndexField, IndexSpec, Lens};
use integration_test::actions::{
    ConfigureMigration, DeleteIndex, GetCollections, ListIndexes, NewEncryptedIndex, NewIndex,
    SetActiveCollectionVersion, Truncate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, CHUNKS};

const REFUSED: &str =
    "Article__body_chunk holds the chunks of Article.body and is changed only through Article";

const GENERATED_INDEX: &str = "Article__body_chunk__sourceID_ASC";

fn index_on(field: &str, unique: bool) -> IndexSpec {
    IndexSpec {
        unique,
        fields: vec![IndexField {
            name: field.into(),
            descending: false,
        }],
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.protect.admin_ops")]
fn index_changes_to_the_chunk_collection_are_refused() {
    TestCase::from_actions(vec![
        add_article_collection(),
        NewIndex {
            collection: CHUNKS.into(),
            index: index_on("hash", true),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        DeleteIndex {
            collection: CHUNKS.into(),
            index_name: GENERATED_INDEX.into(),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        NewEncryptedIndex {
            collection: CHUNKS.into(),
            field_name: "text".into(),
            index_type: "equality".into(),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        ListIndexes {
            collection: CHUNKS.into(),
            expected_data: json!({ "indexes": [
                m::object(json!({ "Description": m::object(json!({ "Name": GENERATED_INDEX })) })),
            ] }),
            ..Default::default()
        }
        .into(),
        NewIndex {
            collection: "Article".into(),
            index: index_on("title", false),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.protect.admin_ops")]
fn collection_changes_to_the_chunk_collection_are_refused() {
    TestCase::from_actions(vec![
        add_article_collection(),
        GetCollections {
            collection_name: Some(CHUNKS.into()),
            common: Common {
                capture: captures([("chunks", "/collections/0/VersionID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Truncate {
            collection: CHUNKS.into(),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        SetActiveCollectionVersion {
            version_id: Text::reference("chunks"),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        ConfigureMigration {
            source_version_id: Text::reference("chunks"),
            destination_version_id: "bafyreinotachunkversion".into(),
            lens: Lens::default(),
            expected_error: Some(REFUSED.into()),
            ..Default::default()
        }
        .into(),
        Truncate {
            collection: "Article".into(),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
