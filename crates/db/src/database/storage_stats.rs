//! Read-only logical byte accounting. Counts are not compressed disk usage.

use std::collections::BTreeMap;

use defra_core::{Block, CrdtDelta, Signature};
use serde::Serialize;
use storage::corekv::{IterOptions, Key, Reader, Store};
use storage::keys::{doc_id_index::BlockCIDToDocIDKey, utils::decode_uvarint_ascending};

use crate::{error::Result, DB};

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct ByteCounts {
    pub keys: u64,
    pub key_bytes: u64,
    pub value_bytes: u64,
}

impl ByteCounts {
    fn record(&mut self, key: &[u8], value: &[u8]) {
        self.keys += 1;
        self.key_bytes += key.len() as u64;
        self.value_bytes += value.len() as u64;
    }
}

#[derive(Debug, Default, Serialize)]
pub struct FieldStats {
    pub blocks: ByteCounts,
    /// Populated only when document-owner counting was requested.
    pub documents: Option<u64>,
    pub versions: Option<u64>,
    pub max_versions_per_document: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct CollectionStats {
    pub names: Vec<String>,
    pub datastore: ByteCounts,
    pub blocks: ByteCounts,
    pub fields: BTreeMap<String, FieldStats>,
}

#[derive(Debug, Default, Serialize)]
pub struct StorageStats {
    pub total: ByteCounts,
    pub stores: BTreeMap<String, ByteCounts>,
    /// Keyed by collection ID, not by a mutable collection name.
    pub collections: BTreeMap<String, CollectionStats>,
    pub block_kinds: BTreeMap<String, ByteCounts>,
    pub unattributed_datastore: ByteCounts,
    pub unattributed_blocks: ByteCounts,
}

impl<S: Store> DB<S> {
    /// Inspect one consistent read snapshot. Returns aggregates, never values.
    pub async fn storage_stats(&self, count_versions: bool) -> Result<StorageStats> {
        let txn = self.store().new_txn(true).await?;
        let result = collect(txn.as_ref(), count_versions).await;
        txn.discard();
        result
    }
}

/// Inspect a root-store reader. Use a snapshot so both passes see the same data.
/// Document-version counting retains one counter per (collection, field, document).
pub async fn collect(reader: &dyn Reader, count_versions: bool) -> Result<StorageStats> {
    let mut report = StorageStats::default();
    let mut versions = BTreeMap::new();
    let mut short_ids = BTreeMap::new();
    let mut layouts: BTreeMap<u32, Vec<schema::IndexDescription>> = BTreeMap::new();
    let mut schemas = reader
        .iterator(IterOptions::new().with_prefix(b"s/collection/id/".to_vec()))
        .await?;
    while let Some(pair) = schemas.next().await? {
        let schema: schema::CollectionVersion =
            serde_json::from_slice(&pair.value).map_err(|_| {
                crate::Error::Other("invalid stored collection schema during storage scan".into())
            })?;
        let key = [
            b"s".as_slice(),
            &storage::keys::systemstore::CollectionID::new(&schema.collection_id).bytes(),
        ]
        .concat();
        if let Some(value) = reader.get(&key).await? {
            if let Some(id) = std::str::from_utf8(&value)
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
            {
                short_ids.insert(id, schema.collection_id.clone());
                layouts
                    .entry(id)
                    .or_default()
                    .extend(schema.indexes.iter().cloned());
            }
        }
        let entry = report
            .collections
            .entry(schema.collection_id.clone())
            .or_default();
        if !entry.names.contains(&schema.name) {
            entry.names.push(schema.name);
            entry.names.sort();
        }
        versions.insert(schema.version_id, schema.collection_id);
    }
    schemas.close().await?;

    let mut document_versions: BTreeMap<(String, String), BTreeMap<String, u64>> = BTreeMap::new();
    let mut iter = reader.iterator(IterOptions::new()).await?;
    while let Some(pair) = iter.next().await? {
        report.total.record(&pair.key, &pair.value);
        let namespace = match pair.key.first() {
            Some(b'd') => "datastore",
            Some(b'b') => "blockstore",
            Some(b'h') => "headstore",
            Some(b's') => "systemstore",
            Some(b'p') => "peerstore",
            Some(b'e') => "encstore",
            Some(b'a') => "acpstore",
            _ => "other",
        };
        report
            .stores
            .entry(namespace.into())
            .or_default()
            .record(&pair.key, &pair.value);
        if let Some(key) = pair.key.strip_prefix(b"d") {
            let collection = text_collection(key)
                .filter(|id| report.collections.contains_key(*id))
                .or_else(|| {
                    datastore_collection(key, &layouts)
                        .and_then(|id| short_ids.get(&id))
                        .map(String::as_str)
                });
            if let Some(id) = collection {
                report
                    .collections
                    .get_mut(id)
                    .expect("loaded collection")
                    .datastore
                    .record(&pair.key, &pair.value);
            } else {
                report.unattributed_datastore.record(&pair.key, &pair.value);
            }
        }
        if let Some(key) = pair.key.strip_prefix(b"b") {
            let Ok(cid) = cid::Cid::try_from(key) else {
                report.unattributed_blocks.record(&pair.key, &pair.value);
                continue;
            };
            let block = Block::from_dag_cbor(&pair.value).ok();
            let kind = match block.as_ref().map(|block| &block.delta) {
                Some(CrdtDelta::Lww(_)) => "lww",
                Some(CrdtDelta::Counter(_)) => "counter",
                Some(CrdtDelta::Composite(_)) => "composite",
                Some(CrdtDelta::Collection(_)) => "collection",
                Some(CrdtDelta::CollectionSet(_)) => "collection_set",
                Some(CrdtDelta::FieldDefinition(_)) => "field_definition",
                Some(CrdtDelta::CollectionDefinition(_)) => "collection_definition",
                None if Signature::from_dag_cbor(&pair.value).is_ok() => "signature",
                _ => "other",
            };
            report
                .block_kinds
                .entry(kind.into())
                .or_default()
                .record(&pair.key, &pair.value);
            let collection = block
                .as_ref()
                .and_then(|block| block.delta.schema_version_id())
                .and_then(|id| versions.get(id));
            if let (Some(id), Some(block)) = (collection, block) {
                let stats = report.collections.get_mut(id).expect("loaded collection");
                stats.blocks.record(&pair.key, &pair.value);
                let field = match &block.delta {
                    CrdtDelta::Lww(delta) => Some(&delta.field_name),
                    CrdtDelta::Counter(delta) => Some(&delta.field_name),
                    _ => None,
                };
                if let Some(field) = field {
                    stats
                        .fields
                        .entry(field.clone())
                        .or_default()
                        .blocks
                        .record(&pair.key, &pair.value);
                    if count_versions {
                        let prefix = [
                            b"s".as_slice(),
                            &BlockCIDToDocIDKey::block_prefix(&cid.to_string()),
                        ]
                        .concat();
                        let mut owners = reader
                            .iterator(
                                IterOptions::new()
                                    .with_prefix(prefix.clone())
                                    .with_keys_only(true),
                            )
                            .await?;
                        let counts = document_versions
                            .entry((id.clone(), field.clone()))
                            .or_default();
                        while let Some(owner) = owners.next().await? {
                            if let Ok(doc) = std::str::from_utf8(&owner.key[prefix.len()..]) {
                                *counts.entry(doc.to_owned()).or_default() += 1;
                            }
                        }
                        owners.close().await?;
                    }
                }
            } else {
                report.unattributed_blocks.record(&pair.key, &pair.value);
            }
        }
        if report.total.keys % 1024 == 0 {
            // A one-shot waker yield rather than tokio::task::yield_now: the
            // wasm client builds this crate without the native feature, which
            // is the only thing that links tokio in.
            futures::future::poll_fn(|cx| {
                cx.waker().wake_by_ref();
                std::task::Poll::<()>::Pending
            })
            .await;
        }
    }
    iter.close().await?;
    for ((collection, field), counts) in document_versions {
        let stats = report
            .collections
            .get_mut(&collection)
            .expect("loaded collection")
            .fields
            .get_mut(&field)
            .expect("counted field");
        stats.documents = Some(counts.len() as u64);
        stats.versions = Some(counts.values().sum());
        stats.max_versions_per_document = Some(counts.values().copied().max().unwrap_or(0));
    }
    Ok(report)
}

fn text_collection(key: &[u8]) -> Option<&str> {
    let key = key
        .strip_prefix(b"/d/")
        .or_else(|| key.strip_prefix(b"/v/"))
        .or_else(|| key.strip_prefix(b"/se/"))
        .or_else(|| key.strip_prefix(b"/del/"))?;
    let end = key.iter().position(|byte| *byte == b'/')?;
    std::str::from_utf8(&key[..end]).ok()
}

fn datastore_collection(
    key: &[u8],
    layouts: &BTreeMap<u32, Vec<schema::IndexDescription>>,
) -> Option<u32> {
    if let Some(key) = key.strip_prefix(b"/collection/vi/") {
        let (rest, id) = decode_uvarint_ascending(key).ok()?;
        rest.starts_with(b"/").then_some(())?;
        return u32::try_from(id).ok();
    }
    let key = key.strip_prefix(b"/")?;
    let vector = vector_collection(key, layouts);
    let ordinary = decode_uvarint_ascending(key)
        .ok()
        .and_then(|(rest, id)| Some((u32::try_from(id).ok()?, rest.strip_prefix(b"/")?)));
    if let Some(vector) = vector {
        // The two integer encodings overlap. An ordered index can even have
        // exactly the same key as a vector record. Leave overlapping known
        // index prefixes unattributed, independent of field type or uniqueness.
        if ordinary
            .is_some_and(|(id, rest)| id != vector && ordered_index_prefix(rest, id, layouts))
        {
            return None;
        }
        return Some(vector);
    }
    ordinary.map(|(id, _)| id)
}

fn vector_integer(key: &[u8]) -> Option<(&[u8], u64)> {
    let (rest, id) = storage::encoding::decode_uvarint_ascending(key).ok()?;
    let encoded = storage::encoding::encode_uvarint_ascending(Vec::new(), id);
    key.starts_with(&encoded).then_some((rest, id))
}

fn vector_collection(
    key: &[u8],
    layouts: &BTreeMap<u32, Vec<schema::IndexDescription>>,
) -> Option<u32> {
    let (rest, collection) = vector_integer(key)?;
    let collection = u32::try_from(collection).ok()?;
    let (rest, index) = vector_integer(rest.strip_prefix(b"/")?)?;
    rest.strip_prefix(b"/")?;
    // The prefix owns every epoch and auxiliary kind, including future kinds.
    layouts
        .get(&collection)?
        .iter()
        .any(|desc| u64::from(desc.id) == index && desc.is_vector())
        .then_some(collection)
}

fn ordered_index_prefix(
    key: &[u8],
    collection: u32,
    layouts: &BTreeMap<u32, Vec<schema::IndexDescription>>,
) -> bool {
    let Ok((rest, index)) = decode_uvarint_ascending(key) else {
        return false;
    };
    rest.starts_with(b"/")
        && layouts.get(&collection).is_some_and(|indexes| {
            indexes
                .iter()
                .any(|desc| u64::from(desc.id) == index && !desc.is_vector())
        })
}
