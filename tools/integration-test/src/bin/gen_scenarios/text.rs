//! Whitespace handling for the SDL and query strings of a scenario.

/// Args whose multi-line text is dedented into a readable raw string.
pub const DEDENTED: &[&str] = &["sdl", "query", "baseQuery", "optimizedQuery"];

fn leading_width(line: &str) -> usize {
    let mut width = 0;
    for c in line.chars() {
        match c {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}

/// Splits on newlines, expands leading tabs to four spaces, drops a blank
/// first line and a whitespace-only last line, and dedents by the minimum
/// indent of the non-blank lines. Blank lines become empty.
pub fn dedent(text: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    if lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| leading_width(l))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                return String::new();
            }
            let content = line.trim_start_matches([' ', '\t']);
            format!("{}{content}", " ".repeat(leading_width(line) - indent))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Plain control characters other than newline and tab cannot sit in a raw
/// Rust string, and CR is normalized by the compiler.
pub fn raw_safe(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
}
