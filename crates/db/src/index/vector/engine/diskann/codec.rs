//! Byte layout of DiskANN's private keyspace, little-endian throughout.
//!
//! ```text
//! Trained  1 version  4 dimensions  4 m
//! Graph    1 version  1 has entry  8 entry  8 live  8 generation
//! Record   m own code  4 neighbor count  per neighbor: 8 id, m code
//! Pending  m code of a vector inserted since the last merge
//! Deleted  empty: a tombstone awaiting consolidation
//! ```
//!
//! A record carries its neighbours' codes so one read ranks every edge out of
//! a node: the layout of AiSAQ and LM-DiskANN, and the reason the walk needs
//! nothing resident.
//!
//! With resident codes the record holds only ids and each node's code has its
//! own key, which a search loads into memory once:
//!
//! ```text
//! Record   4 neighbor count  per neighbor: 8 id
//! Code     m code of a node in the graph
//! ```
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
pub const CODE: u8 = b'c';

const VERSION: u8 = 0x01;
const TRAINED_LEN: usize = 1 + 4 + 4;
const GRAPH_STATE_LEN: usize = 1 + 1 + 8 + 8 + 8;

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
    /// Merges applied, so a resident code cache knows when to reload.
    pub generation: u64,
}

/// Both halves together, as [`DiskAnn::state`](super::DiskAnn::state) returns them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub dimensions: u32,
    pub m: u32,
    pub entry: Option<NodeId>,
    pub live: u64,
    pub generation: u64,
}

impl State {
    pub fn new(trained: Trained, graph: GraphState) -> Self {
        Self {
            dimensions: trained.dimensions,
            m: trained.m,
            entry: graph.entry,
            live: graph.live,
            generation: graph.generation,
        }
    }

    pub fn graph(&self) -> GraphState {
        GraphState {
            entry: self.entry,
            live: self.live,
            generation: self.generation,
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
    buf.extend_from_slice(&state.generation.to_le_bytes());
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
        generation: u64_at(18),
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

/// A record's own code and its edges, borrowed from the record's bytes.
pub struct RecordEdges<'bytes> {
    pub code: &'bytes [u8],
    edges: std::slice::ChunksExact<'bytes, u8>,
}

impl<'bytes> Iterator for RecordEdges<'bytes> {
    type Item = (NodeId, &'bytes [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.edges.next()?;
        Some((
            NodeId(u64::from_le_bytes(chunk[..8].try_into().unwrap())),
            &chunk[8..],
        ))
    }
}

/// Every length is checked before it is used, so a corrupt record is an error
/// rather than a panic or a huge allocation.
pub fn record_edges(bytes: &[u8], m: usize) -> Result<RecordEdges<'_>> {
    let header = m + 4;
    if bytes.len() < header {
        return Err(invalid("DISKANN record is truncated"));
    }
    let count = u32::from_le_bytes(bytes[m..header].try_into().unwrap()) as usize;
    let entry = 8 + m;
    if bytes.len() != header + count.saturating_mul(entry) {
        return Err(invalid("DISKANN record length disagrees with its count"));
    }
    Ok(RecordEdges {
        code: &bytes[..m],
        edges: bytes[header..].chunks_exact(entry),
    })
}

pub fn decode_record(bytes: &[u8], m: usize) -> Result<Record> {
    let edges = record_edges(bytes, m)?;
    Ok(Record {
        code: edges.code.to_vec(),
        neighbors: edges.map(|(id, code)| (id, code.to_vec())).collect(),
    })
}

/// A resident-codes record: the neighbours' ids, their codes left behind.
pub fn encode_ids(neighbors: &[(NodeId, Vec<u8>)]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + neighbors.len() * 8);
    buf.extend_from_slice(&(neighbors.len() as u32).to_le_bytes());
    for (id, _) in neighbors {
        buf.extend_from_slice(&id.0.to_le_bytes());
    }
    buf
}

pub fn decode_ids(bytes: &[u8]) -> Result<Vec<NodeId>> {
    let Some((count, ids)) = bytes.split_first_chunk::<4>() else {
        return Err(invalid("DISKANN record is truncated"));
    };
    if ids.len() != (u32::from_le_bytes(*count) as usize).saturating_mul(8) {
        return Err(invalid("DISKANN record length disagrees with its count"));
    }
    Ok(ids
        .as_chunks::<8>()
        .0
        .iter()
        .map(|id| NodeId(u64::from_le_bytes(*id)))
        .collect())
}

/// The neighbour ids of a record in either layout.
pub fn neighbor_ids(bytes: &[u8], m: usize, resident_codes: bool) -> Result<Vec<NodeId>> {
    if resident_codes {
        return decode_ids(bytes);
    }
    Ok(decode_record(bytes, m)?
        .neighbors
        .into_iter()
        .map(|(id, _)| id)
        .collect())
}

fn invalid(message: &str) -> Error {
    Error::Other(format!("vector index: {message}"))
}
