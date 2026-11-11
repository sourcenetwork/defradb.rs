use base64::Engine;
use cid::Cid;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, ImmutableDeltaPayload, LwwDeltaPayload,
    Signature, SignatureHeader, SignatureType,
};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use serde_json::Value;
use std::collections::BTreeMap;

use super::{ClientUpdateError, ClientUpdateSnapshot};

/// Reconstruct and check the canonical blocks returned by an authenticated
/// `_commits` query. The caller must request the complete unfiltered history.
impl ClientUpdateSnapshot {
    pub fn from_commits(doc_id: &str, rows: &[Value]) -> Result<Self, ClientUpdateError> {
        let mut groups: RapidHashMap<Cid, Vec<&Value>> = RapidHashMap::new();
        for row in rows {
            let doc = row
                .get("docID")
                .and_then(Value::as_str)
                .ok_or(ClientUpdateError::InvalidSnapshot)?;
            let cid: Cid = row
                .get("cid")
                .and_then(Value::as_str)
                .ok_or(ClientUpdateError::InvalidSnapshot)?
                .parse()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            if doc != doc_id {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            groups.entry(cid).or_default().push(row);
        }
        let mut blocks = Vec::with_capacity(groups.len());
        let mut signatures = RapidHashSet::new();
        let mut composites = RapidHashSet::new();
        let mut composite_parents = RapidHashSet::new();
        for (cid, rows) in groups {
            let row = rows[0];
            let text = |name: &str| {
                row.get(name)
                    .and_then(Value::as_str)
                    .ok_or(ClientUpdateError::InvalidSnapshot)
            };
            for other in &rows[1..] {
                for name in [
                    "height",
                    "collectionVersionId",
                    "heads",
                    "links",
                    "signature",
                ] {
                    if row.get(name) != other.get(name) {
                        return Err(ClientUpdateError::InvalidSnapshot);
                    }
                }
            }
            let priority = row
                .get("height")
                .and_then(Value::as_u64)
                .ok_or(ClientUpdateError::InvalidSnapshot)?;
            let version = text("collectionVersionId")?.to_owned();
            let name = text("fieldName")?;
            let heads = parse_heads(row.get("heads").ok_or(ClientUpdateError::InvalidSnapshot)?)?;
            let links = parse_links(row.get("links").ok_or(ClientUpdateError::InvalidSnapshot)?)?;
            let signature = parse_signature(
                row.get("signature")
                    .ok_or(ClientUpdateError::InvalidSnapshot)?,
            )?;
            let signature_cid = signature.as_ref().map(|(cid, _)| *cid);
            let block = if name == "_C" {
                if rows.len() != 1 {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                composites.insert(cid);
                composite_parents.extend(heads.iter().copied());
                Block::new_with_options(
                    CrdtDelta::Composite(CompositeDeltaPayload {
                        collection_version_id: version.clone(),
                        priority,
                        status: 1,
                    }),
                    heads.clone(),
                    links.clone(),
                    None,
                    signature_cid,
                )
            } else {
                if !links.is_empty() {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                let mut fields = BTreeMap::new();
                for item in &rows {
                    let field = item
                        .get("fieldName")
                        .and_then(Value::as_str)
                        .ok_or(ClientUpdateError::InvalidSnapshot)?;
                    let data = parse_data(
                        item.get("delta")
                            .ok_or(ClientUpdateError::InvalidSnapshot)?,
                    )?;
                    if fields
                        .insert(field.to_owned(), serde_bytes::ByteBuf::from(data))
                        .is_some()
                    {
                        return Err(ClientUpdateError::InvalidSnapshot);
                    }
                }
                let lww = if fields.len() == 1 {
                    let (field, data) = fields
                        .first_key_value()
                        .ok_or(ClientUpdateError::InvalidSnapshot)?;
                    Some(Block::new_with_options(
                        CrdtDelta::Lww(LwwDeltaPayload {
                            field_name: field.clone(),
                            priority,
                            collection_version_id: version.clone(),
                            data: data.to_vec(),
                        }),
                        heads.clone(),
                        Vec::new(),
                        None,
                        signature_cid,
                    ))
                } else {
                    None
                };
                if lww.as_ref().and_then(|block| block.generate_cid().ok()) == Some(cid) {
                    lww.ok_or(ClientUpdateError::InvalidSnapshot)?
                } else {
                    if !heads.is_empty() || priority != 1 {
                        return Err(ClientUpdateError::InvalidSnapshot);
                    }
                    Block::new_with_options(
                        CrdtDelta::Immutable(ImmutableDeltaPayload {
                            priority,
                            collection_version_id: version,
                            fields,
                        }),
                        Vec::new(),
                        Vec::new(),
                        None,
                        signature_cid,
                    )
                }
            };
            if block
                .generate_cid()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?
                != cid
            {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            if matches!(block.delta, CrdtDelta::Immutable(_)) {
                let (sig_cid, encoded) = signature.ok_or(ClientUpdateError::InvalidSnapshot)?;
                if signatures.insert(sig_cid) {
                    blocks.push(encoded);
                }
            }
            blocks.push(
                block
                    .to_dag_cbor()
                    .map_err(|_| ClientUpdateError::InvalidSnapshot)?,
            );
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

fn parse_data(value: &Value) -> Result<Vec<u8>, ClientUpdateError> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(value) => base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| ClientUpdateError::InvalidSnapshot),
        _ => Err(ClientUpdateError::InvalidSnapshot),
    }
}

fn parse_signature(value: &Value) -> Result<Option<(Cid, Vec<u8>)>, ClientUpdateError> {
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
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text("value")?)
        .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
    let signature = Signature::new(
        SignatureHeader::new(kind, text("identity")?.as_bytes().to_vec()),
        bytes,
    );
    let cid = signature
        .generate_cid()
        .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
    let encoded = signature
        .to_dag_cbor()
        .map_err(|_| ClientUpdateError::InvalidSnapshot)?;
    Ok(Some((cid, encoded)))
}
