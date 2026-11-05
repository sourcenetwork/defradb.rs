use base64::Engine;
use cid::Cid;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload, Signature, SignatureHeader,
    SignatureType,
};
use rapidhash::{HashSetExt, RapidHashSet};
use serde_json::Value;

use super::{ClientUpdateError, ClientUpdateSnapshot};

/// Reconstruct and check the canonical blocks returned by an authenticated
/// `_commits` query. The caller must request the complete unfiltered history.
impl ClientUpdateSnapshot {
    pub fn from_commits(doc_id: &str, rows: &[Value]) -> Result<Self, ClientUpdateError> {
        let mut blocks = Vec::with_capacity(rows.len());
        let mut seen = RapidHashSet::new();
        let mut composites = RapidHashSet::new();
        let mut composite_parents = RapidHashSet::new();
        for row in rows {
            let text = |name: &str| {
                row.get(name)
                    .and_then(Value::as_str)
                    .ok_or(ClientUpdateError::InvalidSnapshot)
            };
            if text("docID")? != doc_id {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            let cid: Cid = text("cid")?
                .parse()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            if !seen.insert(cid) {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            let priority = row
                .get("height")
                .and_then(Value::as_u64)
                .ok_or(ClientUpdateError::InvalidSnapshot)?;
            let version = text("collectionVersionId")?.to_owned();
            let name = text("fieldName")?;
            let heads = parse_heads(row.get("heads").ok_or(ClientUpdateError::InvalidSnapshot)?)?;
            let links = parse_links(row.get("links").ok_or(ClientUpdateError::InvalidSnapshot)?)?;
            let delta = if name == "_C" {
                composites.insert(cid);
                composite_parents.extend(heads.iter().copied());
                CrdtDelta::Composite(CompositeDeltaPayload {
                    collection_version_id: version,
                    priority,
                    status: 1,
                })
            } else {
                if !links.is_empty() {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                let data = match row.get("delta") {
                    Some(Value::Null) => Vec::new(),
                    Some(Value::String(value)) => base64::engine::general_purpose::STANDARD
                        .decode(value)
                        .map_err(|_| ClientUpdateError::InvalidSnapshot)?,
                    _ => return Err(ClientUpdateError::InvalidSnapshot),
                };
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: name.to_owned(),
                    priority,
                    collection_version_id: version,
                    data,
                })
            };
            let mut block = Block::new(delta, heads, links);
            block.signature = parse_signature(
                row.get("signature")
                    .ok_or(ClientUpdateError::InvalidSnapshot)?,
            )?;
            let encoded = block
                .to_dag_cbor()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            if block
                .generate_cid()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?
                != cid
            {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            blocks.push(encoded);
        }
        let heads = composites
            .difference(&composite_parents)
            .copied()
            .collect::<Vec<_>>();
        Self::from_blocks(doc_id, &blocks, &heads)
    }
}

fn parse_heads(value: &Value) -> Result<Vec<Cid>, ClientUpdateError> {
    value
        .as_array()
        .ok_or(ClientUpdateError::InvalidSnapshot)?
        .iter()
        .map(|item| {
            item.get("cid")
                .and_then(Value::as_str)
                .ok_or(ClientUpdateError::InvalidSnapshot)?
                .parse()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)
        })
        .collect()
}

fn parse_links(value: &Value) -> Result<Vec<DAGLink>, ClientUpdateError> {
    value
        .as_array()
        .ok_or(ClientUpdateError::InvalidSnapshot)?
        .iter()
        .map(|item| {
            let cid: Cid = item
                .get("cid")
                .and_then(Value::as_str)
                .ok_or(ClientUpdateError::InvalidSnapshot)?
                .parse()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            let name = item
                .get("fieldName")
                .and_then(Value::as_str)
                .ok_or(ClientUpdateError::InvalidSnapshot)?;
            Ok(DAGLink::new(name, cid))
        })
        .collect()
}

fn parse_signature(value: &Value) -> Result<Option<Cid>, ClientUpdateError> {
    if value.is_null() {
        return Ok(None);
    }
    let text = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ClientUpdateError::InvalidSnapshot)
    };
    let kind = match text("type")? {
        "EdDSA" => SignatureType::EdDSA,
        "ES256K" => SignatureType::ES256K,
        "ES256" => SignatureType::ES256,
        "BLS_AUG_V1" => SignatureType::BLSAugV1,
        _ => return Err(ClientUpdateError::InvalidSnapshot),
    };
    let signature = base64::engine::general_purpose::STANDARD
        .decode(text("value")?)
        .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
    Signature::new(
        SignatureHeader::new(kind, text("identity")?.as_bytes().to_vec()),
        signature,
    )
    .generate_cid()
    .map(Some)
    .map_err(|_| ClientUpdateError::InvalidSnapshot)
}
