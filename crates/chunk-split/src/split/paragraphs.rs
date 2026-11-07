//! Splits a text into paragraphs.

/// A paragraph, with the blank lines that follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Para<'a> {
    pub text: &'a str,
    /// Where it starts in the whole text, in characters.
    pub start: usize,
    /// Its length in characters.
    pub chars: usize,
    /// Whether it is a heading (`# Title`, or a line underlined with `===` or `---`).
    pub heading: bool,
    /// Its characters without the surrounding whitespace.
    pub weight: usize,
}

struct Open {
    start: usize,
    has_text: bool,
    // Set after a blank line or a heading, so the next text line starts a new paragraph.
    closed: bool,
    heading: bool,
}

/// Splits `text` into paragraphs that join back into `text` exactly. Blank lines
/// stay with the paragraph before them, and a heading is always its own paragraph.
pub fn paragraphs(text: &str) -> Vec<Para<'_>> {
    let mut paras = Vec::new();
    let mut open: Option<Open> = None;
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        let blank = line.trim().is_empty();
        open = Some(match open {
            None => Open {
                start: pos,
                has_text: !blank,
                closed: !blank && is_atx_heading(line),
                heading: !blank && is_atx_heading(line),
            },
            Some(mut cur) if blank => {
                cur.closed |= cur.has_text;
                cur
            }
            Some(mut cur)
                if cur.has_text && !cur.closed && !cur.heading && is_setext_underline(line) =>
            {
                cur.heading = true;
                cur.closed = true;
                cur
            }
            Some(cur) if cur.has_text && (cur.closed || is_atx_heading(line)) => {
                push(&mut paras, text, cur.start, pos, cur.heading);
                let atx = is_atx_heading(line);
                Open {
                    start: pos,
                    has_text: true,
                    closed: atx,
                    heading: atx,
                }
            }
            Some(mut cur) => {
                let atx = !cur.has_text && is_atx_heading(line);
                cur.has_text = true;
                cur.closed |= atx;
                cur.heading |= atx;
                cur
            }
        });
        pos += line.len();
    }
    if let Some(cur) = open {
        push(&mut paras, text, cur.start, pos, cur.heading);
    }
    paras
}

fn push<'a>(paras: &mut Vec<Para<'a>>, text: &'a str, from: usize, to: usize, heading: bool) {
    let part = &text[from..to];
    let start = paras.last().map_or(0, |p: &Para<'_>| p.start + p.chars);
    paras.push(Para {
        text: part,
        start,
        chars: part.chars().count(),
        heading,
        weight: part.trim().chars().count(),
    });
}

fn is_atx_heading(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    indent <= 3
        && (1..=6).contains(&hashes)
        && rest[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

fn is_setext_underline(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.len() >= 2 && trimmed.bytes().all(|b| b == b'='))
        || (trimmed.len() >= 3 && trimmed.bytes().all(|b| b == b'-'))
}
