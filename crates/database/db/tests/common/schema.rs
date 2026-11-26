use schema::CollectionVersion;
use schema::DistanceMetric;
use schema::FieldDescription;
use schema::FieldKind;
use schema::HnswParams;
use schema::IndexKind;
use schema::VectorAlgorithm;
use schema::VectorIndexDescription;

/// The version `sdl` defines first, exactly as `DB::add_collection` registers
/// it, so documents and blocks built from it carry the registered IDs.
pub fn parsed(sdl: &str) -> CollectionVersion {
    query::parse_sdl(sdl).unwrap().remove(0)
}

pub const USERS_SDL: &str = "type Users { name: String }";

pub fn test_schema() -> CollectionVersion {
    parsed(USERS_SDL)
}

pub fn users_schema() -> CollectionVersion {
    parsed(USERS_SDL)
}

pub const COLLECTION_SHORT_ID: u32 = 1;
pub const DIMENSIONS: u32 = 4;

pub const DOCS_SDL: &str = "type docs { title: String embedding: [Float!] }";

pub fn docs_schema() -> CollectionVersion {
    parsed(DOCS_SDL)
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

pub const TEST_COLLECTION_SDL: &str = "type TestDoc { x: Int }";

pub fn test_collection() -> CollectionVersion {
    parsed(TEST_COLLECTION_SDL)
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
