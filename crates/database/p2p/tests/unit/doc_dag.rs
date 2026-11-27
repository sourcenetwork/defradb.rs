//! A document DAG built from real composite and field blocks, for tests of
//! what a fetch serves.

use cid::Cid;
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};
use rapidhash::RapidHashMap;

/// One branch of a document: its composite heads and each field's heads.
#[derive(Clone, Default)]
pub(crate) struct DocDag {
    heads: Vec<(Cid, u64)>,
    field_heads: RapidHashMap<String, Vec<(Cid, u64)>>,
}

fn encode(block: &Block) -> (Cid, Vec<u8>) {
    (
        block.generate_cid().expect("block cid"),
        block.to_dag_cbor().expect("block bytes"),
    )
}

fn next_priority(heads: &[(Cid, u64)]) -> u64 {
    heads
        .iter()
        .map(|(_, priority)| *priority)
        .max()
        .unwrap_or(0)
        + 1
}

impl DocDag {
    /// One update setting `fields`: a field block per field, then the
    /// composite over them. Returns the blocks, composite last.
    pub(crate) fn update(&mut self, fields: &[&str], tag: u32) -> Vec<(Cid, Vec<u8>)> {
        let mut blocks = Vec::new();
        let mut links = Vec::new();
        for field in fields {
            let heads = self.field_heads.remove(*field).unwrap_or_default();
            let priority = next_priority(&heads);
            let block = Block::new(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: (*field).to_string(),
                    priority,
                    collection_version_id: "cv".to_string(),
                    data: tag.to_be_bytes().to_vec(),
                }),
                heads.iter().map(|(cid, _)| *cid).collect(),
                vec![],
            );
            let (cid, data) = encode(&block);
            self.field_heads
                .insert((*field).to_string(), vec![(cid, priority)]);
            links.push(DAGLink::new(*field, cid));
            blocks.push((cid, data));
        }
        let priority = next_priority(&self.heads);
        let block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "cv".to_string(),
                priority,
                status: 1,
            }),
            self.heads.iter().map(|(cid, _)| *cid).collect(),
            links,
        );
        let (cid, data) = encode(&block);
        self.heads = vec![(cid, priority)];
        blocks.push((cid, data));
        blocks
    }

    /// The branch that sees both `self` and `other`, before its next update.
    pub(crate) fn merged(&self, other: &DocDag) -> DocDag {
        let mut merged = self.clone();
        for head in &other.heads {
            if !merged.heads.contains(head) {
                merged.heads.push(*head);
            }
        }
        for (field, heads) in &other.field_heads {
            let entry = merged.field_heads.entry(field.clone()).or_default();
            for head in heads {
                if !entry.contains(head) {
                    entry.push(*head);
                }
            }
        }
        merged
    }

    pub(crate) fn heads(&self) -> Vec<Cid> {
        self.heads.iter().map(|(cid, _)| *cid).collect()
    }
}

/// `count` updates alternating between two fields.
pub(crate) fn updates(dag: &mut DocDag, count: u32, tag: u32) -> Vec<(Cid, Vec<u8>)> {
    let mut blocks = Vec::new();
    for idx in 0..count {
        let fields: &[&str] = if idx % 2 == 0 {
            &["name", "age"]
        } else {
            &["name"]
        };
        blocks.extend(dag.update(fields, tag * 100_000 + idx));
    }
    blocks
}
