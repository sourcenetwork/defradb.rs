//! Groups paragraphs into units and units into segments, so an edit that keeps
//! the fence before its unit only changes its own segment's chunks.

use sha2::{Digest, Sha256};

use super::paragraphs::Para;

/// How long a segment is on average, counted in chunks of `max_chars`.
pub const SEGMENT_CHUNKS: u128 = 4;

/// A heading with the paragraph under it, or a paragraph or heading on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit<'p, 'a> {
    pub paras: &'p [Para<'a>],
    /// Its paragraphs' characters without the surrounding whitespace.
    pub weight: usize,
    /// The first 8 bytes of the hash of its words.
    pub roll: u64,
}

/// Splits `paras` into units. Only a heading followed by text joins the next paragraph.
pub fn units<'p, 'a>(paras: &'p [Para<'a>]) -> Vec<Unit<'p, 'a>> {
    let mut units = Vec::new();
    let mut start = 0;
    for (i, para) in paras.iter().enumerate() {
        let joins_next = para.heading && paras.get(i + 1).is_some_and(|next| !next.heading);
        if !joins_next {
            units.push(unit(&paras[start..=i]));
            start = i + 1;
        }
    }
    units
}

fn unit<'p, 'a>(paras: &'p [Para<'a>]) -> Unit<'p, 'a> {
    // Hash only the words, so adding or removing a blank line doesn't move a fence.
    let mut hasher = Sha256::new();
    for para in paras {
        hasher.update(para.text.trim().as_bytes());
        hasher.update(b"\n");
    }
    let mut first = [0; 8];
    first.copy_from_slice(&hasher.finalize()[..8]);
    Unit {
        paras,
        weight: paras.iter().map(|p| p.weight).sum(),
        roll: u64::from_be_bytes(first),
    }
}

/// Whether a new segment starts before `unit`. It reads only the unit, so an
/// edit can only move the fence before its own unit.
pub fn fence_before(unit: &Unit<'_, '_>, max: usize) -> bool {
    // Every character has the same chance, so segments follow the text's size.
    u128::from(unit.roll) * SEGMENT_CHUNKS * (max as u128) < (unit.weight as u128) << 64
}

/// Cuts `paras` into segments, each a run of whole units.
pub fn segments<'p, 'a>(paras: &'p [Para<'a>], max: usize) -> Vec<&'p [Para<'a>]> {
    let mut segments = Vec::new();
    let (mut start, mut at) = (0, 0);
    for unit in units(paras) {
        if at > 0 && fence_before(&unit, max) {
            segments.push(&paras[start..at]);
            start = at;
        }
        at += unit.paras.len();
    }
    if start < paras.len() {
        segments.push(&paras[start..]);
    }
    segments
}
