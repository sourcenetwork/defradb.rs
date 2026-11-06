//! Groups paragraphs into segments, so an edit that keeps its fences only
//! changes its own segment's chunks.

use super::paragraphs::Para;

/// How long a segment is on average, counted in chunks of `max_chars`.
pub const SEGMENT_CHUNKS: u128 = 4;

/// Whether a new segment starts before `cur`.
///
/// Every character of `cur` has the same small chance to start a segment, so a
/// segment averages `SEGMENT_CHUNKS` times `max` characters whatever the paragraph sizes.
/// It reads only `cur` and whether `prev` is a heading, so an edit can only move
/// the fence before its own paragraph, and a heading keeps its text.
pub fn fence_before(prev: &Para<'_>, cur: &Para<'_>, max: usize) -> bool {
    !prev.heading
        && u128::from(cur.roll) * SEGMENT_CHUNKS * (max as u128) < (cur.weight as u128) << 64
}

pub(super) fn segments<'p, 'a>(paras: &'p [Para<'a>], max: usize) -> Vec<&'p [Para<'a>]> {
    let mut segments = Vec::new();
    let mut start = 0;
    for i in 1..paras.len() {
        if fence_before(&paras[i - 1], &paras[i], max) {
            segments.push(&paras[start..i]);
            start = i;
        }
    }
    if start < paras.len() {
        segments.push(&paras[start..]);
    }
    segments
}
