use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexDescription,
    IndexedFieldDescription,
};
use serde_json::json;

use crate::error::QueryError;
use crate::mapper::{Field, Filter, Select};
use crate::planner::builder::Planner;

fn filter(value: serde_json::Value) -> Filter {
    Filter::from_conditions(value.as_object().cloned().unwrap())
}

fn json_blob(indexed: bool) -> CollectionVersion {
    let collection = CollectionVersion::new(
        "jsonBlob",
        "v1",
        "coll-json",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "jsonField", FieldKind::json(), CType::LwwRegister),
        ],
    );
    if !indexed {
        return collection;
    }
    collection.with_index(IndexDescription {
        id: 1,
        name: "jsonBlob_jsonField_ASC".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "jsonField".to_string(),
            descending: false,
        }],
    })
}

fn users() -> CollectionVersion {
    CollectionVersion::new(
        "users",
        "v1",
        "coll-users",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "meta", FieldKind::json(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "posts",
                FieldKind::relation("posts", true),
                CType::LwwRegister,
            )
            .with_relation_name("author_posts"),
        ],
    )
}

fn posts() -> CollectionVersion {
    CollectionVersion::new(
        "posts",
        "v1",
        "coll-posts",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "title", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "author",
                FieldKind::relation("users", false),
                CType::LwwRegister,
            )
            .with_relation_name("author_posts")
            .as_primary(),
            FieldDescription::new("4", "_authorID", FieldKind::doc_id(), CType::LwwRegister)
                .with_relation_name("author_posts")
                .as_primary(),
        ],
    )
}

fn sub_path_filter() -> Filter {
    filter(json!({"jsonField": {"i": {"love": {"_like": "%family%"}}}}))
}

#[test]
fn json_sub_path_is_not_a_relation_path() {
    let planner = Planner::new(vec![json_blob(false)]);
    let collection = planner.collection("jsonBlob").unwrap();

    let filter = sub_path_filter();

    assert_eq!(
        filter.get_multi_level_relation_paths(),
        vec![vec!["jsonField".to_string(), "i".to_string()]]
    );
    assert!(planner
        .relation_filter_paths(&filter, collection)
        .is_empty());
}

#[test]
fn relation_path_stops_at_json_field() {
    let planner = Planner::new(vec![users(), posts()]);
    let collection = planner.collection("posts").unwrap();

    let into_json = filter(json!({"author": {"meta": {"a": {"b": {"_eq": 1}}}}}));
    assert!(planner
        .relation_filter_paths(&into_json, collection)
        .is_empty());

    let through_relations = filter(json!({"author": {"posts": {"title": {"_eq": "x"}}}}));
    assert_eq!(
        planner.relation_filter_paths(&through_relations, collection),
        vec![vec!["author".to_string(), "posts".to_string()]]
    );
}

#[test]
fn json_sub_path_filter_plans_without_index() {
    workers::block_on(async {
        let planner = Planner::new(vec![json_blob(false)]);
        let select = Select::new("jsonBlob")
            .with_field(Field::new("jsonField"))
            .with_filter(sub_path_filter());

        let result = planner.plan_with_index_info(&select).unwrap();

        assert!(!result.uses_index());
    })
    .unwrap()
}

#[test]
fn json_sub_path_filter_uses_json_index() {
    workers::block_on(async {
        let planner = Planner::new(vec![json_blob(true)]);
        let select = Select::new("jsonBlob")
            .with_field(Field::new("jsonField"))
            .with_filter(sub_path_filter());

        let result = planner.plan_with_index_info(&select).unwrap();

        assert!(result.uses_index());
        assert_eq!(
            result.index_scan.as_ref().unwrap().index_name,
            "jsonBlob_jsonField_ASC"
        );
    })
    .unwrap()
}

#[test]
fn json_sub_path_filter_through_relation_plans() {
    workers::block_on(async {
        let planner = Planner::new(vec![users(), posts()]);
        let select = Select::new("posts")
            .with_field(Field::new("title"))
            .with_filter(filter(
                json!({"author": {"meta": {"a": {"b": {"_eq": 1}}}}}),
            ));

        planner.plan(&select).unwrap();
    })
    .unwrap()
}

#[test]
fn sub_selection_on_json_field_is_go_validation_error() {
    workers::block_on(async {
        let planner = Planner::new(vec![json_blob(false)]);
        let nested = Select::new("jsonField")
            .with_field_name("jsonField")
            .with_field(Field::new("i"));
        let select = Select::new("jsonBlob").with_select(nested);

        let err = planner.plan(&select).err().unwrap();

        assert!(matches!(err, QueryError::SubSelectionOnLeaf { .. }));
        assert_eq!(
            err.to_string(),
            "Field \"jsonField\" of type \"JSON\" must not have a sub selection."
        );
    })
    .unwrap()
}
