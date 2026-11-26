//! A new index that repeats a listed index's definition is refused (#3320);
//! `ensure_ordinary_index` keeps defining its stable name beside one.

use std::sync::Arc;

use db::DB;
use schema::{HnswParams, IndexKind, IndexedFieldDescription, VectorIndexDescription};
use storage::RegolithStore;

use crate::common::schema::{vector_kind, DOCS_SDL};
use crate::create::{by_name, open_with_collection};

fn embedding() -> Vec<IndexedFieldDescription> {
    vec![IndexedFieldDescription {
        name: "embedding".to_string(),
        descending: false,
    }]
}

#[test]
fn a_repeated_ordered_index_is_refused_and_nothing_is_defined() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("Users").await;
        let (fields, kind) = by_name();
        db.create_index("Users", None, fields.clone(), kind)
            .await
            .unwrap();

        let err = db
            .create_index("Users", Some("again"), fields, kind)
            .await
            .expect_err("a copy must be refused");

        assert_eq!(
            err.to_string(),
            "index with the same definition already exists. Name: Users_name_ASC"
        );
        let names: Vec<String> = db
            .require_collection("Users")
            .unwrap()
            .get_indexes()
            .iter()
            .map(|index| index.name.clone())
            .collect();
        assert_eq!(names, ["Users_name_ASC"]);
    })
    .unwrap()
}

#[test]
fn a_vector_index_repeated_with_unset_defaults_is_refused() {
    workers::block_on(async {
        let db = Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), DOCS_SDL)
            .await
            .unwrap();
        db.create_index("docs", Some("by_embedding"), embedding(), vector_kind())
            .await
            .unwrap();
        let IndexKind::Vector(defaults) = vector_kind() else {
            unreachable!("vector_kind is a vector index")
        };
        let unset = VectorIndexDescription {
            hnsw: Some(HnswParams {
                m: 0,
                ef_construction: 0,
                ef_search: 0,
            }),
            ..defaults
        };

        let err = db
            .create_index("docs", None, embedding(), IndexKind::Vector(unset))
            .await
            .expect_err("the same vector index with its defaults unset must be refused");

        assert_eq!(
            err.to_string(),
            "index with the same definition already exists. Name: by_embedding"
        );
    })
    .unwrap()
}

#[test]
fn a_vector_index_with_other_parameters_is_created() {
    workers::block_on(async {
        let db = Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), DOCS_SDL)
            .await
            .unwrap();
        db.create_index("docs", Some("by_embedding"), embedding(), vector_kind())
            .await
            .unwrap();
        let IndexKind::Vector(defaults) = vector_kind() else {
            unreachable!("vector_kind is a vector index")
        };
        let wider = VectorIndexDescription {
            hnsw: Some(HnswParams {
                ef_search: HnswParams::default().ef_search + 1,
                ..HnswParams::default()
            }),
            ..defaults
        };

        db.create_index(
            "docs",
            Some("by_embedding_wide"),
            embedding(),
            IndexKind::Vector(wider),
        )
        .await
        .unwrap();
    })
    .unwrap()
}

#[test]
fn ensure_ordinary_index_defines_its_name_beside_a_same_definition_index() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("Users").await;
        let (fields, kind) = by_name();
        db.create_index("Users", None, fields.clone(), kind)
            .await
            .unwrap();

        let ensured = db
            .ensure_ordinary_index("Users", "by_name", fields)
            .await
            .unwrap();

        assert_eq!(ensured.name, "by_name");
        let mut names: Vec<String> = db
            .require_collection("Users")
            .unwrap()
            .get_indexes()
            .iter()
            .map(|index| index.name.clone())
            .collect();
        names.sort();
        assert_eq!(names, ["Users_name_ASC", "by_name"]);
    })
    .unwrap()
}
