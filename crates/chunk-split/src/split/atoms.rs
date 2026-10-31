//! Breaks a paragraph into pieces small enough for one chunk.

const SENTENCE_END: [char; 3] = ['.', '!', '?'];
const CLOSERS: [char; 7] = ['"', '\'', ')', ']', '\u{201D}', '\u{2019}', '\u{00BB}'];

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
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if !SENTENCE_END.contains(&c) {
            continue;
        }
        while chars.next_if(|&(_, c)| CLOSERS.contains(&c)).is_some() {}
        let mut saw_space = false;
        while chars.next_if(|&(_, c)| c.is_whitespace()).is_some() {
            saw_space = true;
        }
        if saw_space {
            let end = chars.peek().map_or(text.len(), |&(i, _)| i);
            pieces.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

/// Splits `text` after each run of spaces. The pieces join back into `text` exactly.
pub fn cut_words(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c.is_whitespace() && chars.peek().is_some_and(|&(_, next)| !next.is_whitespace()) {
            let end = chars.peek().map_or(text.len(), |&(i, _)| i);
            pieces.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

fn cut_chars(text: &str) -> Vec<&str> {
    text.char_indices()
        .map(|(i, c)| &text[i..i + c.len_utf8()])
        .collect()
}
