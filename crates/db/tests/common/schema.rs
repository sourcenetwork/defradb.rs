use schema::CollectionVersion;
use schema::DistanceMetric;
use schema::FieldDescription;
use schema::FieldKind;
use schema::HnswParams;
use schema::IndexKind;
use schema::VectorAlgorithm;
use schema::VectorIndexDescription;

pub fn test_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "v1",
        "col-users",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

pub fn users_schema() -> CollectionVersion {
    let mut schema = CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    );
    schema.is_materialized = true;
    schema
}

pub const COLLECTION_SHORT_ID: u32 = 1;
pub const DIMENSIONS: u32 = 4;

pub fn docs_schema() -> CollectionVersion {
    CollectionVersion::new(
        "docs",
        "v1",
        "col-docs",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "title",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "3",
                "embedding",
                FieldKind::float64_array(),
                schema::CType::LwwRegister,
            ),
        ],
    )
}

pub fn vector_kind() -> IndexKind {
    IndexKind::Vector(VectorIndexDescription {
        algorithm: VectorAlgorithm::Hnsw,
        metric: DistanceMetric::Cosine,
        dimensions: DIMENSIONS,
        hnsw: Some(HnswParams::default()),
        ivfpq: None,
        ivfflat: None,
        ssg: None,
        diskann: None,
    })
}

pub fn test_collection() -> CollectionVersion {
    CollectionVersion::new(
        "TestDoc",
        "v1",
        "col-test-doc",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "x", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
}

/// A collection at `version_id` with no fields, for building blocks of a
/// document whose fields are all mutable.
pub fn version(version_id: &str) -> CollectionVersion {
    CollectionVersion::new("Test", version_id, "col-test", vec![])
}

/// A collection defining `fields` as last-write-wins scalars.
pub fn open_collection(fields: &[&str]) -> CollectionVersion {
    fields
        .iter()
        .enumerate()
        .fold(
            schema::CollectionBuilder::new("Test", "bafytest"),
            |builder, (i, name)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *name,
                    FieldKind::Scalar(schema::ScalarKind::None),
                    schema::CType::LwwRegister,
                ))
            },
        )
        .build()
}

/// A document built from `json`, against a collection defining its keys.
pub fn doc_from_json(json: &str) -> document::Document {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let names: Vec<&str> = value
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    document::Document::from_json_str(json, open_collection(&names)).unwrap()
}
