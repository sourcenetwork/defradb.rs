//! Repeats the last sentences of a chunk at the start of the next, so each chunk
//! carries some context.

use super::atoms::sentence_ends;

/// A chunk repeats at most `max_chars / REPEAT_SHARE` characters, so the
/// repeat stays context and never the bulk of the chunk.
pub const REPEAT_SHARE: usize = 4;

// Repeat fewer sentences when all of them would not fit in `max`.
pub(super) fn with_overlap(cores: &[String], max: usize, overlap: usize) -> Vec<&str> {
    let mut prefixes = Vec::with_capacity(cores.len());
    let mut prev: Option<&str> = None;
    for core in cores {
        prefixes.push(match prev {
            Some(prev) if overlap > 0 => borrow(prev, core.chars().count(), max, overlap),
            _ => "",
        });
        prev = Some(core);
    }
    prefixes
}

fn borrow(prev: &str, core_chars: usize, max: usize, overlap: usize) -> &str {
    let ends = sentence_ends(prev);
    // A trailing list or half sentence is no sentence, and only what touches the core is context.
    if ends.last() != Some(&prev.len()) {
        return "";
    }
    // The first sentence may have begun in the chunk before, so it is never repeated.
    let starts = &ends[..ends.len() - 1];
    let cap = (max / REPEAT_SHARE).min(core_chars);
    starts[starts.len().saturating_sub(overlap)..]
        .iter()
        .map(|&from| &prev[from..])
        .find(|candidate| {
            let len = candidate.chars().count();
            len <= cap && len + core_chars <= max
        })
        .unwrap_or("")
}
