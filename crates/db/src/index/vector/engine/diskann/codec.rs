//! Byte layout of DiskANN's private keyspace, little-endian throughout.
//!
//! ```text
//! Trained  1 version  4 dimensions  4 m
//! Graph    1 version  1 has entry  8 entry  8 live
//! Record   m own code  4 neighbor count  per neighbor: 8 id, m code
//! Pending  m code of a vector inserted since the last merge
//! Deleted  empty: a tombstone with a graph record, awaiting consolidation
//! ```
//!
//! A record carries its neighbours' codes so one read ranks every edge out of
//! a node: the layout of AiSAQ and LM-DiskANN, and the reason the walk needs
//! nothing resident.
//!
//! What training fixes and what a merge changes are separate keys: an insert
//! reads only the first, so a merge committing under it cannot abort it.

use crate::index::error::{Error, Result};
use crate::index::vector::store::NodeId;

pub const TRAINED: u8 = b't';
pub const GRAPH_STATE: u8 = b's';
pub const CODEBOOK: u8 = b'b';
pub const GRAPH: u8 = b'g';
pub const DELETED: u8 = b'd';
pub const PENDING: u8 = b'p';

const VERSION: u8 = 0x01;
const TRAINED_LEN: usize = 1 + 4 + 4;
const GRAPH_STATE_LEN: usize = 1 + 1 + 8 + 8;

/// What training fixed: the width and the quantizer's subspace count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trained {
    pub dimensions: u32,
    pub m: u32,
}

/// What merges change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphState {
    /// Where every walk starts; `None` while the graph is empty.
    pub entry: Option<NodeId>,
    /// Graph records written and not consolidated away.
    pub live: u64,
}

/// Both halves, for a caller that wants the whole picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub dimensions: u32,
    pub m: u32,
    pub entry: Option<NodeId>,
    pub live: u64,
}

impl State {
    pub fn new(trained: Trained, graph: GraphState) -> Self {
        Self {
            dimensions: trained.dimensions,
            m: trained.m,
            entry: graph.entry,
            live: graph.live,
        }
    }

    pub fn trained(&self) -> Trained {
        Trained {
            dimensions: self.dimensions,
            m: self.m,
        }
    }

    pub fn graph(&self) -> GraphState {
        GraphState {
            entry: self.entry,
            live: self.live,
        }
    }
}

pub fn encode_trained(trained: &Trained) -> Vec<u8> {
    let mut buf = Vec::with_capacity(TRAINED_LEN);
    buf.push(VERSION);
    buf.extend_from_slice(&trained.dimensions.to_le_bytes());
    buf.extend_from_slice(&trained.m.to_le_bytes());
    buf
}

pub fn decode_trained(bytes: &[u8]) -> Result<Trained> {
    if bytes.len() != TRAINED_LEN || bytes[0] != VERSION {
        return Err(invalid("unsupported DISKANN trained-state encoding"));
    }
    Ok(Trained {
        dimensions: u32::from_le_bytes(bytes[1..5].try_into().unwrap()),
        m: u32::from_le_bytes(bytes[5..9].try_into().unwrap()),
    })
}

pub fn encode_graph_state(state: &GraphState) -> Vec<u8> {
    let mut buf = Vec::with_capacity(GRAPH_STATE_LEN);
    buf.push(VERSION);
    buf.push(u8::from(state.entry.is_some()));
    buf.extend_from_slice(&state.entry.map_or(0, |id| id.0).to_le_bytes());
    buf.extend_from_slice(&state.live.to_le_bytes());
    buf
}

pub fn decode_graph_state(bytes: &[u8]) -> Result<GraphState> {
    if bytes.len() != GRAPH_STATE_LEN || bytes[0] != VERSION {
        return Err(invalid("unsupported DISKANN graph-state encoding"));
    }
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    Ok(GraphState {
        entry: (bytes[1] != 0).then(|| NodeId(u64_at(2))),
        live: u64_at(10),
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
