//! Breaks a paragraph into pieces small enough for one chunk.

use unicode_segmentation::UnicodeSegmentation;

const SENTENCE_END: [char; 3] = ['.', '!', '?'];
// Chinese and Japanese put no space after these, so they end a sentence on their own.
const WIDE_SENTENCE_END: [char; 5] = ['\u{3002}', '\u{FF01}', '\u{FF1F}', '\u{FF0E}', '\u{FF61}'];
const CLOSERS: [char; 15] = [
    '"', '\'', ')', ']', '\u{201D}', '\u{2019}', '\u{00BB}', '\u{300D}', '\u{300F}', '\u{FF09}',
    '\u{FF3D}', '\u{3011}', '\u{300B}', '\u{3009}', '\u{FF02}',
];

pub(super) fn atoms(text: &str, max: usize) -> Vec<&str> {
    let mut out = Vec::new();
    if text.chars().count() <= max {
        if !text.is_empty() {
            out.push(text);
        }
    } else {
        refine(text, max, 0, &mut out);
    }
    out
}

// Cut at the most natural boundary that works, so chunks stay readable.
fn refine<'a>(text: &'a str, max: usize, level: usize, out: &mut Vec<&'a str>) {
    let pieces = match level {
        0 => cut_sentences(text),
        1 => cut_words(text),
        2 => text.graphemes(true).collect(),
        _ => cut_chars(text),
    };
    for piece in pieces {
        if piece.is_empty() {
            continue;
        }
        if piece.chars().count() <= max {
            out.push(piece);
        } else {
            refine(piece, max, level + 1, out);
        }
    }
}

/// Splits `text` after each sentence. The pieces join back into `text` exactly.
pub fn cut_sentences(text: &str) -> Vec<&str> {
    cut_at(text, sentence_ends(text))
}

/// Where each sentence of `text` ends, in bytes. The last is `text.len()` only
/// when the text ends with a whole sentence.
pub(super) fn sentence_ends(text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut graphemes = text.grapheme_indices(true).peekable();
    while let Some((_, g)) = graphemes.next() {
        let wide = WIDE_SENTENCE_END.contains(&lead(g));
        if !wide && !SENTENCE_END.contains(&lead(g)) {
            continue;
        }
        while graphemes
            .next_if(|&(_, g)| CLOSERS.contains(&lead(g)))
            .is_some()
        {}
        let mut saw_space = false;
        while graphemes
            .next_if(|&(_, g)| lead(g).is_whitespace())
            .is_some()
        {
            saw_space = true;
        }
        // Without a space, an ASCII dot may sit inside a word like `v1.2`.
        if wide || saw_space {
            ends.push(graphemes.peek().map_or(text.len(), |&(i, _)| i));
        }
    }
    ends
}

/// Splits `text` after each run of spaces. The pieces join back into `text` exactly.
pub fn cut_words(text: &str) -> Vec<&str> {
    let mut ends = Vec::new();
    let mut graphemes = text.grapheme_indices(true).peekable();
    while let Some((_, g)) = graphemes.next() {
        if let Some(&(i, next)) = graphemes.peek() {
            if lead(g).is_whitespace() && !lead(next).is_whitespace() {
                ends.push(i);
            }
        }
    }
    cut_at(text, ends)
}

fn cut_at(text: &str, ends: Vec<usize>) -> Vec<&str> {
    let mut pieces = Vec::with_capacity(ends.len() + 1);
    let mut start = 0;
    for end in ends {
        pieces.push(&text[start..end]);
        start = end;
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

// A grapheme is read by its first character, so a mark on a space is still a space.
fn lead(grapheme: &str) -> char {
    grapheme.chars().next().unwrap_or_default()
}

// Only for a grapheme longer than a whole chunk, which cannot stay whole.
fn cut_chars(text: &str) -> Vec<&str> {
    text.char_indices()
        .map(|(i, c)| &text[i..i + c.len_utf8()])
        .collect()
}
