//! HNSW additions change only adjacency; structural node puts terminate them.
//! Counter operands never replace entry-point/height/rebuild metadata.
use super::{decode, encode, VECTOR_COUNTS, VECTOR_LINKS};
use crate::vector::{decode_meta, decode_node, encode_meta, encode_node, NodeId};

pub fn links(layer: u32, neighbor: u64) -> Vec<u8> {
    encode(
        VECTOR_LINKS,
        &[layer.to_be_bytes().as_slice(), &neighbor.to_be_bytes()].concat(),
    )
}
pub fn counts(live: i64, waste: u64) -> Vec<u8> {
    encode(
        VECTOR_COUNTS,
        &[live.to_be_bytes(), waste.to_be_bytes()].concat(),
    )
}
fn additions(bytes: &[u8]) -> Option<Vec<(u32, u64)>> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(12) {
        return None;
    }
    bytes
        .as_chunks::<12>()
        .0
        .iter()
        .map(|entry| {
            Some((
                u32::from_be_bytes(entry[..4].try_into().ok()?),
                u64::from_be_bytes(entry[4..].try_into().ok()?),
            ))
        })
        .collect()
}
pub(super) fn full(kind: u8, base: Option<&[u8]>, operands: &[&[u8]]) -> Option<Vec<u8>> {
    match kind {
        VECTOR_LINKS => {
            let mut node = decode_node(base?).ok()?;
            for operand in operands {
                let (VECTOR_LINKS, bytes) = decode(operand)? else {
                    return None;
                };
                for (layer, neighbor) in additions(bytes)? {
                    // Raising a node's height is structural, never a blind addition.
                    let neighbors = node.layers.get_mut(layer as usize)?;
                    neighbors.retain(|id| *id != node.id);
                    if neighbor != node.id.0 {
                        neighbors.push(NodeId(neighbor));
                    }
                    neighbors.sort_unstable();
                    neighbors.dedup();
                }
            }
            Some(encode_node(&node))
        }
        VECTOR_COUNTS => {
            let mut meta = decode_meta(base?).ok()?;
            for operand in operands {
                let (VECTOR_COUNTS, bytes) = decode(operand)? else {
                    return None;
                };
                if bytes.len() != 16 {
                    return None;
                }
                let live = i64::from_be_bytes(bytes[..8].try_into().ok()?);
                let waste = u64::from_be_bytes(bytes[8..].try_into().ok()?);
                // These are approximate rebuild counters; preserve their existing
                // saturating deletion behavior and never wrap on overflow.
                meta.live = meta.live.saturating_add_signed(live);
                meta.waste = meta.waste.saturating_add(waste);
            }
            Some(encode_meta(&meta))
        }
        _ => None,
    }
}
pub(super) fn fold(left: &[u8], right: &[u8]) -> Option<Vec<u8>> {
    let (VECTOR_LINKS, left) = decode(left)? else {
        return None;
    };
    let (VECTOR_LINKS, right) = decode(right)? else {
        return None;
    };
    additions(left)?;
    additions(right)?;
    Some(encode(VECTOR_LINKS, &[left, right].concat()))
}
pub(super) fn touches(kind: u8, bytes: &[u8], parts: &[u32]) -> bool {
    match kind {
        VECTOR_LINKS if additions(bytes).is_some() => parts.contains(&1),
        VECTOR_COUNTS if bytes.len() == 16 => parts.contains(&1),
        _ => true,
    }
}
