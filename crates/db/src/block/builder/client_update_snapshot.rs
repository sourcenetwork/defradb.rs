use cid::Cid;
use defra_core::block::{generate_cid_from_bytes, Block, CrdtDelta};
use document::DocID;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use super::ClientUpdateError;

/// A client-verified unencrypted LWW history for one physical document.
///
/// The caller supplies canonical block bytes and its observed composite heads.
/// This checks CID integrity, complete ancestry to the DocID's genesis and
/// field-head derivation. A server withholding a newer head cannot cause a
/// different graph to be signed: the later signing wave must match this one.
pub struct ClientUpdateSnapshot {
    pub(super) doc_id: String,
    pub(super) versions: RapidHashSet<String>,
    pub(super) heads: RapidHashMap<String, Vec<Cid>>,
    pub(super) priorities: RapidHashMap<String, u64>,
    pub(super) values: RapidHashMap<String, (u64, Vec<u8>)>,
    pub(super) max_priority: u64,
}

impl ClientUpdateSnapshot {
    /// Verify every supplied block and derive live field heads from the
    /// composite history rooted in `doc_id`.
    pub fn from_blocks(
        doc_id: &str,
        blocks: &[Vec<u8>],
        composite_heads: &[Cid],
    ) -> Result<Self, ClientUpdateError> {
        DocID::from_string(doc_id).map_err(|_| ClientUpdateError::InvalidSnapshot)?;
        if composite_heads.is_empty()
            || composite_heads.iter().collect::<RapidHashSet<_>>().len() != composite_heads.len()
        {
            return Err(ClientUpdateError::InvalidSnapshot);
        }
        let mut by_cid = RapidHashMap::new();
        for bytes in blocks {
            let block =
                Block::from_dag_cbor(bytes).map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            if block
                .to_dag_cbor()
                .map_err(|_| ClientUpdateError::InvalidSnapshot)?
                != *bytes
                || block.encryption.is_some()
            {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            let cid =
                generate_cid_from_bytes(bytes).map_err(|_| ClientUpdateError::InvalidSnapshot)?;
            if by_cid.insert(cid, block).is_some() {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
        }
        let mut versions = RapidHashSet::new();
        let mut visited = RapidHashSet::new();
        let mut fields = Vec::new();
        let mut pending = composite_heads.to_vec();
        let mut found_genesis = false;
        while let Some(cid) = pending.pop() {
            if !visited.insert(cid) {
                continue;
            }
            let block = by_cid.get(&cid).ok_or(ClientUpdateError::InvalidSnapshot)?;
            let CrdtDelta::Composite(delta) = &block.delta else {
                return Err(ClientUpdateError::InvalidSnapshot);
            };
            if delta.status != 1 {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            versions.insert(delta.collection_version_id.clone());
            let parents = block.heads.as_deref().unwrap_or_default();
            if parents.is_empty() {
                if DocID::new_v0(cid).to_string() != doc_id {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                found_genesis = true;
            } else {
                for parent in parents {
                    let previous = by_cid
                        .get(parent)
                        .ok_or(ClientUpdateError::InvalidSnapshot)?;
                    if !matches!(&previous.delta, CrdtDelta::Composite(value) if value.priority < delta.priority)
                    {
                        return Err(ClientUpdateError::InvalidSnapshot);
                    }
                    pending.push(*parent);
                }
            }
            for link in block.links.as_deref().unwrap_or_default() {
                fields.push((link.name.clone(), link.link));
            }
        }
        if !found_genesis {
            return Err(ClientUpdateError::InvalidSnapshot);
        }
        let mut field_blocks = RapidHashMap::new();
        let mut superseded = RapidHashSet::new();
        while let Some((name, cid)) = fields.pop() {
            if let Some(existing) = field_blocks.get(&cid) {
                if existing != &name {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                continue;
            }
            let block = by_cid.get(&cid).ok_or(ClientUpdateError::InvalidSnapshot)?;
            let CrdtDelta::Lww(delta) = &block.delta else {
                return Err(ClientUpdateError::InvalidSnapshot);
            };
            if delta.field_name != name || block.links.is_some() {
                return Err(ClientUpdateError::InvalidSnapshot);
            }
            versions.insert(delta.collection_version_id.clone());
            for parent in block.heads.as_deref().unwrap_or_default() {
                let previous = by_cid
                    .get(parent)
                    .ok_or(ClientUpdateError::InvalidSnapshot)?;
                if !matches!(&previous.delta, CrdtDelta::Lww(value) if value.field_name == name && value.priority < delta.priority)
                {
                    return Err(ClientUpdateError::InvalidSnapshot);
                }
                superseded.insert(*parent);
                fields.push((name.clone(), *parent));
            }
            field_blocks.insert(cid, name);
        }
        let mut heads = RapidHashMap::new();
        let mut priorities = RapidHashMap::new();
        let mut max_priority = 0;
        let mut values: RapidHashMap<String, (u64, Vec<u8>)> = RapidHashMap::new();
        for (cid, name) in field_blocks {
            if superseded.contains(&cid) {
                continue;
            }
            let CrdtDelta::Lww(delta) = &by_cid[&cid].delta else {
                return Err(ClientUpdateError::InvalidSnapshot);
            };
            let priority = delta.priority;
            let candidate = (priority, delta.data.clone());
            match values.get_mut(&name) {
                Some(existing) if candidate > *existing => *existing = candidate,
                None => {
                    values.insert(name.clone(), candidate);
                }
                _ => {}
            }
            heads.entry(name.clone()).or_insert_with(Vec::new).push(cid);
            let current = priorities.entry(name).or_insert(0);
            *current = (*current).max(priority);
            max_priority = max_priority.max(priority);
        }
        for cid in composite_heads {
            let block = by_cid.get(cid).ok_or(ClientUpdateError::InvalidSnapshot)?;
            let priority = block.delta.priority();
            max_priority = max_priority.max(priority);
            let current = priorities.entry("C".to_owned()).or_insert(0);
            *current = (*current).max(priority);
        }
        heads.insert("C".to_owned(), composite_heads.to_vec());
        Ok(Self {
            doc_id: doc_id.to_owned(),
            versions,
            heads,
            priorities,
            values,
            max_priority,
        })
    }
}
