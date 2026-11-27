//! Collection version objects with Go's JavaScript wire shape.
//!
//! Go hands a `client.CollectionVersion` to JavaScript through
//! `goji.MarshalJS` (`js/client_store.go`), which is `json.Marshal` followed
//! by `JSON.parse`. The struct carries no JSON tags, so every exported field
//! becomes a key named after it, in declaration order
//! (`client/collection_description.go`, `client/collection_field_description.go`,
//! `client/index.go`). `Kind` and `Typ` keep their numeric wire encodings:
//! only Go's `collection describe` display renders them as strings.
//!
//! These views borrow a stored [`CollectionVersion`] and emit Go's keys, with
//! Go's value for an absent option (`null`). The wasm client is deliberately
//! data-only: its `getCollections` returns these version objects instead of
//! Go's collection handles, which carry the collection methods.

use serde::Serialize;
use wasm_bindgen::JsValue;

use schema::{
    CollectionSetDescription, CollectionSource, CollectionVersion, EncryptedIndexDescription,
    FieldDescription, FieldKind, HnswParams, IndexDescription, IndexKind, IndexedFieldDescription,
    PolicyDescription, QuerySource, VectorEmbeddingDescription,
};

use crate::error::{Result, WasmError};

/// Go's `client.CollectionVersion`.
#[derive(Serialize)]
pub struct CollectionJs<'a> {
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "VersionID")]
    version_id: &'a str,
    #[serde(rename = "CollectionID")]
    collection_id: &'a str,
    #[serde(rename = "CollectionSet")]
    collection_set: Option<&'a CollectionSetDescription>,
    #[serde(rename = "Query")]
    query: Option<QuerySourceJs<'a>>,
    #[serde(rename = "PreviousVersion")]
    previous_version: Option<&'a CollectionSource>,
    #[serde(rename = "Fields")]
    fields: Vec<FieldJs<'a>>,
    #[serde(rename = "Indexes")]
    indexes: Vec<IndexJs<'a>>,
    #[serde(rename = "EncryptedIndexes")]
    encrypted_indexes: &'a [EncryptedIndexDescription],
    #[serde(rename = "Policy")]
    policy: Option<PolicyJs<'a>>,
    #[serde(rename = "IsActive")]
    is_active: bool,
    #[serde(rename = "IsMaterialized")]
    is_materialized: bool,
    #[serde(rename = "IsBranchable")]
    is_branchable: bool,
    #[serde(rename = "IsEmbeddedOnly")]
    is_embedded_only: bool,
    #[serde(rename = "IsPlaceholder")]
    is_placeholder: bool,
    #[serde(rename = "VectorEmbeddings")]
    vector_embeddings: &'a [VectorEmbeddingDescription],
}

/// Go's `client.QuerySource`: `Transform` is an `immutable.Option`, `null` when unset.
#[derive(Serialize)]
struct QuerySourceJs<'a> {
    #[serde(rename = "Query")]
    query: &'a serde_json::Value,
    #[serde(rename = "Transform")]
    transform: Option<&'a str>,
}

/// Go's `client.PolicyDescription`.
#[derive(Serialize)]
struct PolicyJs<'a> {
    #[serde(rename = "ID")]
    id: &'a str,
    #[serde(rename = "ResourceName")]
    resource_name: &'a str,
}

/// Go's `client.CollectionFieldDescription`.
#[derive(Serialize)]
struct FieldJs<'a> {
    #[serde(rename = "FieldID")]
    id: &'a str,
    #[serde(rename = "Name")]
    name: &'a str,
    /// A scalar kind's number, or `{Array, CollectionID}` / `{RelativeID, Array}`
    /// for a relation, as Go's `ScalarKind`, `CollectionKind` and `SelfKind` marshal.
    #[serde(rename = "Kind")]
    kind: &'a FieldKind,
    /// Go's `CType` byte.
    #[serde(rename = "Typ")]
    crdt_type: u8,
    #[serde(rename = "RelationName")]
    relation_name: Option<&'a str>,
    #[serde(rename = "IsPrimary")]
    is_primary: bool,
    #[serde(rename = "DefaultValue")]
    default_value: Option<&'a serde_json::Value>,
    #[serde(rename = "Size")]
    size: usize,
}

/// Go's `client.IndexDescription` as its `MarshalJSON` writes it.
#[derive(Serialize)]
struct IndexJs<'a> {
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Fields")]
    fields: &'a [IndexedFieldDescription],
    #[serde(rename = "Kind")]
    kind: u8,
    #[serde(rename = "KindDescription")]
    kind_description: IndexKindJs<'a>,
    #[serde(rename = "Unique")]
    unique: bool,
}

/// Go's `OrderedIndexDescription` or `VectorIndexDescription`.
#[derive(Serialize)]
#[serde(untagged)]
enum IndexKindJs<'a> {
    Ordered {
        #[serde(rename = "Unique")]
        unique: bool,
    },
    Vector {
        #[serde(rename = "Fields")]
        fields: Vec<&'a str>,
        #[serde(rename = "Algorithm")]
        algorithm: &'static str,
        #[serde(rename = "Metric")]
        metric: schema::DistanceMetric,
        #[serde(rename = "Dimensions")]
        dimensions: u32,
        #[serde(rename = "HNSW", skip_serializing_if = "Option::is_none")]
        hnsw: Option<HnswParams>,
        #[serde(rename = "IVFPQ", skip_serializing_if = "Option::is_none")]
        ivfpq: Option<schema::IvfPqParams>,
        #[serde(rename = "IVFFLAT", skip_serializing_if = "Option::is_none")]
        ivfflat: Option<schema::IvfFlatParams>,
        #[serde(rename = "SSG", skip_serializing_if = "Option::is_none")]
        ssg: Option<schema::SsgParams>,
        #[serde(rename = "DISKANN", skip_serializing_if = "Option::is_none")]
        diskann: Option<schema::DiskAnnParams>,
    },
}

impl<'a> From<&'a CollectionVersion> for CollectionJs<'a> {
    fn from(collection: &'a CollectionVersion) -> Self {
        Self {
            name: &collection.name,
            version_id: &collection.version_id,
            collection_id: &collection.collection_id,
            collection_set: collection.collection_set.as_ref(),
            query: collection.query.as_ref().map(QuerySourceJs::from),
            previous_version: collection.previous_version.as_ref(),
            fields: collection.fields.iter().map(FieldJs::from).collect(),
            indexes: collection.indexes.iter().map(IndexJs::from).collect(),
            encrypted_indexes: &collection.encrypted_indexes,
            policy: collection.policy.as_ref().map(PolicyJs::from),
            is_active: collection.is_active,
            is_materialized: collection.is_materialized,
            is_branchable: collection.is_branchable,
            is_embedded_only: collection.is_embedded_only,
            is_placeholder: collection.is_placeholder,
            vector_embeddings: &collection.vector_embeddings,
        }
    }
}

impl<'a> From<&'a QuerySource> for QuerySourceJs<'a> {
    fn from(source: &'a QuerySource) -> Self {
        Self {
            query: &source.query,
            transform: source.transform.as_deref(),
        }
    }
}

impl<'a> From<&'a PolicyDescription> for PolicyJs<'a> {
    fn from(policy: &'a PolicyDescription) -> Self {
        Self {
            id: &policy.id,
            resource_name: &policy.resource_name,
        }
    }
}

impl<'a> From<&'a FieldDescription> for FieldJs<'a> {
    fn from(field: &'a FieldDescription) -> Self {
        Self {
            id: &field.id,
            name: &field.name,
            kind: &field.kind,
            crdt_type: field.crdt_type.to_u8(),
            relation_name: field.relation_name.as_deref(),
            is_primary: field.is_primary,
            default_value: field.default_value.as_ref(),
            size: field.size,
        }
    }
}

impl<'a> From<&'a IndexDescription> for IndexJs<'a> {
    fn from(index: &'a IndexDescription) -> Self {
        let fields = index.fields.as_slice();
        let (kind, kind_description, unique) = match index.kind {
            None => (
                0,
                IndexKindJs::Ordered {
                    unique: index.unique,
                },
                index.unique,
            ),
            Some(IndexKind::Ordered(ordered)) => (
                0,
                IndexKindJs::Ordered {
                    unique: ordered.unique,
                },
                ordered.unique,
            ),
            Some(IndexKind::Vector(vector)) => (
                1,
                IndexKindJs::Vector {
                    fields: fields.iter().map(|field| field.name.as_str()).collect(),
                    algorithm: vector.algorithm.as_str(),
                    metric: vector.metric,
                    dimensions: vector.dimensions,
                    hnsw: vector.hnsw,
                    ivfpq: vector.ivfpq,
                    ivfflat: vector.ivfflat,
                    ssg: vector.ssg,
                    diskann: vector.diskann,
                },
                false,
            ),
        };
        Self {
            name: &index.name,
            id: index.id,
            fields,
            kind,
            kind_description,
            unique,
        }
    }
}

/// Convert collections to the data-only array this client's `getCollections`
/// returns.
///
/// Serialized as JSON is: options become `null` and relation kinds plain
/// objects, never `undefined` or a `Map`.
pub fn collections_to_js<'a, I>(collections: I) -> Result<JsValue>
where
    I: IntoIterator<Item = &'a CollectionVersion>,
{
    let views: Vec<CollectionJs<'a>> = collections.into_iter().map(CollectionJs::from).collect();
    views
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(WasmError::from)
}
