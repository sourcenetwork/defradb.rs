//! Byte layout of DiskANN's private keyspace, little-endian throughout.
//!
//! ```text
//! State   1 version  4 dimensions  4 m  1 has entry  8 entry  8 live  8 deleted
//! Record  m own code  4 neighbor count  per neighbor: 8 id, m code
//! ```
//!
//! A record carries its neighbours' codes so one read ranks every edge out of
//! a node: the layout of AiSAQ and LM-DiskANN, and the reason the walk needs
//! nothing resident.

use crate::index::error::{Error, Result};
use crate::index::vector::store::NodeId;

pub const STATE: u8 = b's';
pub const CODEBOOK: u8 = b'b';
pub const GRAPH: u8 = b'g';
pub const DELETED: u8 = b'd';

const STATE_VERSION: u8 = 0x01;
const STATE_LEN: usize = 1 + 4 + 4 + 1 + 8 + 8 + 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub dimensions: u32,
    pub m: u32,
    /// Where every walk starts; `None` once consolidation removed the last
    /// node, until the next insert.
    pub entry: Option<NodeId>,
    /// Graph records written and not consolidated away.
    pub live: u64,
    /// Tombstones awaiting consolidation.
    pub deleted: u64,
}

pub fn encode_state(state: &State) -> Vec<u8> {
    let mut buf = Vec::with_capacity(STATE_LEN);
    buf.push(STATE_VERSION);
    buf.extend_from_slice(&state.dimensions.to_le_bytes());
    buf.extend_from_slice(&state.m.to_le_bytes());
    buf.push(u8::from(state.entry.is_some()));
    buf.extend_from_slice(&state.entry.map_or(0, |id| id.0).to_le_bytes());
    buf.extend_from_slice(&state.live.to_le_bytes());
    buf.extend_from_slice(&state.deleted.to_le_bytes());
    buf
}

pub fn decode_state(bytes: &[u8]) -> Result<State> {
    if bytes.len() != STATE_LEN || bytes[0] != STATE_VERSION {
        return Err(invalid("unsupported DISKANN state encoding"));
    }
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    Ok(State {
        dimensions: u32_at(1),
        m: u32_at(5),
        entry: (bytes[9] != 0).then(|| NodeId(u64_at(10))),
        live: u64_at(18),
        deleted: u64_at(26),
    })
}

/// One node's adjacency, with every code the walk needs to rank it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub code: Vec<u8>,
    pub neighbors: Vec<(NodeId, Vec<u8>)>,
}

pub fn record_key(id: NodeId) -> [u8; 8] {
    id.0.to_be_bytes()
}

pub fn id_from_key(key: &[u8]) -> Result<NodeId> {
    key.try_into()
        .map(|bytes| NodeId(u64::from_be_bytes(bytes)))
        .map_err(|_| invalid("DISKANN key is not a node id"))
}

pub fn encode_record(record: &Record) -> Vec<u8> {
    let m = record.code.len();
    let mut buf = Vec::with_capacity(m + 4 + record.neighbors.len() * (8 + m));
    buf.extend_from_slice(&record.code);
    buf.extend_from_slice(&(record.neighbors.len() as u32).to_le_bytes());
    for (id, code) in &record.neighbors {
        buf.extend_from_slice(&id.0.to_le_bytes());
        buf.extend_from_slice(code);
    }
    buf
}

/// Every length is checked before it is used, so a corrupt record is an error
/// rather than a panic or a huge allocation.
pub fn decode_record(bytes: &[u8], m: usize) -> Result<Record> {
    let header = m + 4;
    if bytes.len() < header {
        return Err(invalid("DISKANN record is truncated"));
    }
    let count = u32::from_le_bytes(bytes[m..header].try_into().unwrap()) as usize;
    let entry = 8 + m;
    if bytes.len() != header + count.saturating_mul(entry) {
        return Err(invalid("DISKANN record length disagrees with its count"));
    }
    let neighbors = bytes[header..]
        .chunks_exact(entry)
        .map(|chunk| {
            (
                NodeId(u64::from_le_bytes(chunk[..8].try_into().unwrap())),
                chunk[8..].to_vec(),
            )
        })
        .collect();
    Ok(Record {
        code: bytes[..m].to_vec(),
        neighbors,
    })
}

fn invalid(message: &str) -> Error {
    Error::Other(format!("vector index: {message}"))
}
