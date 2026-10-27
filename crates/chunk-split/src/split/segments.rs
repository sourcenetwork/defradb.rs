//! Groups paragraphs into segments, so an edit that keeps its fences only
//! changes its own segment's chunks.

use super::paragraphs::Para;

/// Whether a new segment starts before `cur`.
///
/// It looks only at `prev` and `cur`, so an edit can't move boundaries elsewhere.
/// A heading starts a segment and the hash never cuts right after one, so it keeps its text.
pub fn fence_before(prev: &Para<'_>, cur: &Para<'_>, max: usize) -> bool {
    cur.heading || cur.chars > max || prev.chars > max || (cur.pick && !prev.heading)
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
