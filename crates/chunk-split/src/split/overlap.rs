//! Repeats the last sentences of a chunk at the start of the next, so each chunk
//! carries some context.

use super::atoms::cut_sentences;

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
    let sentences = cut_sentences(prev);
    let most = overlap.min(sentences.len());
    for take in (1..=most).rev() {
        let from = prev.len()
            - sentences[sentences.len() - take..]
                .iter()
                .map(|s| s.len())
                .sum::<usize>();
        let candidate = &prev[from..];
        if candidate.chars().count() + core_chars <= max {
            return candidate;
        }
    }
    ""
}
