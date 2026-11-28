//! The collection version objects `getCollections` returns, pinned against
//! Go's `json.Marshal` of `client.CollectionVersion`: Go's keys and encodings,
//! in Go's field order. This client returns these data-only objects instead
//! of Go's collection handles.
//!
//! These run natively against the same JSON serializer the wasm client uses.
//! The browser test in `client.rs` checks the parsed JavaScript object.

use schema::{
    CType, CollectionSetDescription, CollectionSource, CollectionVersion,
    EncryptedIndexDescription, FieldDescription, FieldKind, HnswParams, IndexDescription,
    IndexKind, IndexedFieldDescription, OrderedIndexDescription, PolicyDescription, QuerySource,
    ScalarArrayKind, ScalarKind, VectorAlgorithm, VectorEmbeddingDescription,
    VectorIndexDescription,
};

use crate::collection_js::CollectionJs;

const DOC_ID_FIELD: &str = "bafyreihqzhiz3iwro4jozp6kphq4sosg6ccoqcbiaf7rg5dmvea7aux55a";
const USER_ID: &str = "bafyreifccor43tiazq5xp6uzgtihgjf3ivi5btft24vp6rb4vuxg3sf52u";
const AUTHOR_ID: &str = "bafyreibvcavbxqwimz5vdxe5q5href63g3skc6ytg45hm4fqh6wsx57wmq";

fn js_json(collection: &CollectionVersion) -> String {
    serde_json::to_string(&CollectionJs::from(collection)).unwrap()
}

fn doc_id() -> FieldDescription {
    FieldDescription::new(
        DOC_ID_FIELD,
        "_docID",
        FieldKind::Scalar(ScalarKind::DocID),
        CType::None,
    )
}

fn indexed_name() -> Vec<IndexedFieldDescription> {
    vec![IndexedFieldDescription {
        name: "name".to_string(),
        descending: false,
    }]
}

/// `type User { name: String @index age: Int @crdt(type: pcounter) points: Int }`
fn user() -> CollectionVersion {
    let mut user = CollectionVersion::new(
        "User",
        USER_ID,
        USER_ID,
        vec![
            doc_id(),
            FieldDescription::new(
                "bafyreigmjqh4qdxc4q3z6wzg2x6eksmf7hd3ceqn2miv4yqfuta5zpqehi",
                "age",
                FieldKind::Scalar(ScalarKind::NillableInt),
                CType::PCounter,
            ),
            FieldDescription::new(
                "bafyreiaezc5g33yzhyzcgbyiv476lovyztyoliotzksdfogoep5ktgpedq",
                "name",
                FieldKind::Scalar(ScalarKind::NillableString),
                CType::LwwRegister,
            ),
        ],
    );
    user.indexes = vec![IndexDescription {
        name: "User_name_ASC".to_string(),
        id: 1,
        fields: indexed_name(),
        unique: false,
        kind: None,
        auto_generated: false,
    }];
    user
}

#[test]
#[mantra_macros::req_test("defra.wasm.collection_objects")]
fn a_plain_collection_is_gos_js_object() {
    assert_eq!(
        js_json(&user()),
        concat!(
            r#"{"Name":"User","#,
            r#""VersionID":"bafyreifccor43tiazq5xp6uzgtihgjf3ivi5btft24vp6rb4vuxg3sf52u","#,
            r#""CollectionID":"bafyreifccor43tiazq5xp6uzgtihgjf3ivi5btft24vp6rb4vuxg3sf52u","#,
            r#""CollectionSet":null,"Query":null,"PreviousVersion":null,"#,
            r#""Fields":["#,
            r#"{"FieldID":"bafyreihqzhiz3iwro4jozp6kphq4sosg6ccoqcbiaf7rg5dmvea7aux55a","#,
            r#""Name":"_docID","Kind":1,"Typ":0,"RelationName":null,"IsPrimary":false,"#,
            r#""DefaultValue":null,"Size":0},"#,
            r#"{"FieldID":"bafyreigmjqh4qdxc4q3z6wzg2x6eksmf7hd3ceqn2miv4yqfuta5zpqehi","#,
            r#""Name":"age","Kind":4,"Typ":5,"RelationName":null,"IsPrimary":false,"#,
            r#""DefaultValue":null,"Size":0},"#,
            r#"{"FieldID":"bafyreiaezc5g33yzhyzcgbyiv476lovyztyoliotzksdfogoep5ktgpedq","#,
            r#""Name":"name","Kind":11,"Typ":1,"RelationName":null,"IsPrimary":false,"#,
            r#""DefaultValue":null,"Size":0}],"#,
            r#""Indexes":[{"Name":"User_name_ASC","ID":1,"#,
            r#""Fields":[{"Name":"name","Descending":false}],"Kind":0,"#,
            r#""KindDescription":{"Unique":false},"#,
            r#""Unique":false}],"#,
            r#""EncryptedIndexes":[],"Policy":null,"IsActive":true,"IsMaterialized":true,"#,
            r#""IsBranchable":false,"IsEmbeddedOnly":false,"IsPlaceholder":false,"#,
            r#""VectorEmbeddings":[]}"#,
        )
    );
}

#[test]
#[mantra_macros::req_test("defra.wasm.collection_objects")]
fn relation_kinds_are_gos_object_kinds() {
    let author = FieldDescription::new(
        "bafyreic5yfni6cg67fuis6hghkcehecvqex4xa5ru4twahjgucagakrcnu",
        "author",
        FieldKind::Relation {
            collection_id: AUTHOR_ID.to_string(),
            is_array: false,
        },
        CType::None,
    )
    .with_relation_name("author_book")
    .as_primary();
    let manager = FieldDescription::new(
        "",
        "manager",
        FieldKind::SelfRef {
            relative_id: "1".to_string(),
            is_array: true,
        },
        CType::None,
    );
    let book = js_json(&CollectionVersion::new(
        "Book",
        "v",
        "c",
        vec![author, manager],
    ));

    assert!(book.contains(&format!(
        r#""Name":"author","Kind":{{"Array":false,"CollectionID":"{AUTHOR_ID}"}},"Typ":0,"RelationName":"author_book","IsPrimary":true,"#
    )));
    assert!(book.contains(
        r#""Name":"manager","Kind":{"RelativeID":"1","Array":true},"Typ":0,"RelationName":null,"#
    ));
}

#[test]
#[mantra_macros::req_test("defra.wasm.collection_objects")]
fn every_optional_value_is_gos_encoding() {
    let mut points = FieldDescription::new(
        "f1",
        "points",
        FieldKind::ScalarArray(ScalarArrayKind::Float32Array),
        CType::LwwRegister,
    );
    points.default_value = Some(serde_json::json!(3));
    points.size = 2;
    points.immutable = true;

    let mut view = CollectionVersion::new("UserView", "v2", "c", vec![points]);
    view.collection_set = Some(CollectionSetDescription::new("set", 1));
    view.query = Some(QuerySource::new(serde_json::json!({"Name": "User"})));
    view.previous_version = Some(CollectionSource::new("v1"));
    view.encrypted_indexes = vec![EncryptedIndexDescription::new("points")];
    view.policy = Some(PolicyDescription::new("policy", "users"));
    view.vector_embeddings = vec![VectorEmbeddingDescription {
        field_name: "points".to_string(),
        fields: vec!["name".to_string()],
        model: "m".to_string(),
        provider: "p".to_string(),
        template: String::new(),
        url: String::new(),
    }];
    view.governance_root = Some("root".to_string());
    view.is_materialized = false;
    view.is_branchable = true;

    assert_eq!(
        js_json(&view),
        concat!(
            r#"{"Name":"UserView","VersionID":"v2","CollectionID":"c","#,
            r#""CollectionSet":{"CollectionSetID":"set","RelativeID":1},"#,
            r#""Query":{"Query":{"Name":"User"},"Transform":null},"#,
            r#""PreviousVersion":{"SourceCollectionID":"v1","Transform":null},"#,
            r#""Fields":[{"FieldID":"f1","Name":"points","Kind":9,"Typ":1,"#,
            r#""RelationName":null,"IsPrimary":false,"DefaultValue":3,"Size":2}],"#,
            r#""Indexes":[],"#,
            r#""EncryptedIndexes":[{"FieldName":"points","Type":"equality"}],"#,
            r#""Policy":{"ID":"policy","ResourceName":"users"},"#,
            r#""IsActive":true,"IsMaterialized":false,"IsBranchable":true,"#,
            r#""IsEmbeddedOnly":false,"IsPlaceholder":false,"#,
            r#""VectorEmbeddings":[{"FieldName":"points","Fields":["name"],"Model":"m","#,
            r#""Provider":"p","Template":"","URL":""}]}"#,
        )
    );
}

#[test]
#[mantra_macros::req_test("defra.wasm.collection_objects")]
fn index_kinds_are_gos_kind_descriptions() {
    let mut user = user();
    user.indexes = vec![
        IndexDescription {
            name: "unique_name".to_string(),
            id: 1,
            fields: indexed_name(),
            unique: true,
            kind: Some(IndexKind::Ordered(OrderedIndexDescription { unique: true })),
            auto_generated: false,
        },
        IndexDescription {
            name: "embedding".to_string(),
            id: 2,
            fields: vec![IndexedFieldDescription {
                name: "embedding".to_string(),
                descending: false,
            }],
            unique: false,
            kind: Some(IndexKind::Vector(VectorIndexDescription {
                algorithm: VectorAlgorithm::Hnsw,
                dimensions: 3,
                hnsw: Some(HnswParams::default()),
                ..VectorIndexDescription::default()
            })),
            auto_generated: false,
        },
    ];
    assert!(js_json(&user).contains(concat!(
        r#""Indexes":[{"Name":"unique_name","ID":1,"Fields":[{"Name":"name","Descending":false}],"#,
        r#""Kind":0,"KindDescription":{"Unique":true},"Unique":true},"#,
        r#"{"Name":"embedding","ID":2,"Fields":[{"Name":"embedding","Descending":false}],"#,
        r#""Kind":1,"KindDescription":{"Fields":["embedding"],"Algorithm":"HNSW","#,
        r#""Metric":"COSINE","Dimensions":3,"#,
        r#""HNSW":{"M":16,"EfConstruction":128,"EfSearch":64}},"Unique":false}],"#,
    )));
}

#[test]
#[mantra_macros::req_test("defra.wasm.collection_objects")]
fn every_vector_algorithm_keeps_its_parameters() {
    for algorithm in VectorAlgorithm::ALL {
        let mut user = user();
        user.indexes = vec![IndexDescription {
            name: "embedding".to_string(),
            id: 1,
            fields: vec![IndexedFieldDescription {
                name: "embedding".to_string(),
                descending: false,
            }],
            unique: false,
            kind: Some(IndexKind::Vector(VectorIndexDescription {
                algorithm: *algorithm,
                dimensions: 3,
                hnsw: Some(schema::HnswParams::default()),
                ivfpq: Some(schema::IvfPqParams::default()),
                ivfflat: Some(schema::IvfFlatParams::default()),
                ssg: Some(schema::SsgParams::default()),
                diskann: Some(schema::DiskAnnParams::default()),
                ..VectorIndexDescription::default()
            })),
            auto_generated: false,
        }];
        let json: serde_json::Value = serde_json::from_str(&js_json(&user)).unwrap();
        let kind = &json["Indexes"][0]["KindDescription"];
        assert_eq!(kind["Algorithm"], algorithm.as_str());

        let expected = match algorithm {
            VectorAlgorithm::Hnsw => Some("HNSW"),
            VectorAlgorithm::Flat => None,
            VectorAlgorithm::IvfPq => Some("IVFPQ"),
            VectorAlgorithm::IvfFlat => Some("IVFFLAT"),
            VectorAlgorithm::Ssg => Some("SSG"),
            VectorAlgorithm::DiskAnn => Some("DISKANN"),
        };
        for block in ["HNSW", "IVFPQ", "IVFFLAT", "SSG", "DISKANN"] {
            assert_eq!(
                kind.get(block).is_some(),
                expected == Some(block),
                "{algorithm:?} emitted the wrong {block} parameter block: {kind:?}"
            );
        }
    }
}
