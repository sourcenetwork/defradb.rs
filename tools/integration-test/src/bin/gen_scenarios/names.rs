use super::corpus::Loaded;

/// Corpus areas whose name a hand-written test binary already uses.
const COLLIDING: &[&str] = &["acp", "backup", "encryption", "query"];

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
    "try", "typeof", "unsized", "virtual", "yield",
];

pub fn snake(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let mut out = out.trim_matches('_').to_owned();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "t_");
    }
    if KEYWORDS.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// The test binary for a corpus area.
pub fn binary(top: &str) -> String {
    match COLLIDING.contains(&top) {
        true => format!("{top}_scenarios"),
        false => top.to_owned(),
    }
}

/// The file stem for a source file: `simple_test.go` becomes `simple`.
pub fn source_module(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.trim_end_matches(".go");
    snake(name.trim_end_matches("_test"))
}

/// The Go test name in snake case, `Test` prefix dropped, plus the subtest.
pub fn test_fn(loaded: &Loaded) -> String {
    let source = &loaded.scenario.source;
    let test = source.test.strip_prefix("Test").unwrap_or(&source.test);
    let mut words = camel(test);
    if let Some(sub) = &source.subtest {
        words.push('_');
        words.push_str(&camel(sub));
    }
    snake(&words)
}

fn camel(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        let after_lower =
            i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
        let acronym_end = i > 0
            && chars[i - 1].is_ascii_uppercase()
            && chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
        if c.is_ascii_uppercase() && (after_lower || acronym_end) {
            out.push('_');
        }
        out.push(*c);
    }
    out
}

/// Module path from the area down: directories, then the source file.
pub fn modules(loaded: &Loaded) -> Vec<String> {
    let mut modules: Vec<String> = loaded.dir.iter().map(|d| snake(d)).collect();
    modules.push(source_module(&loaded.scenario.source.path));
    modules
}

/// The id a test exports under: its module path with dots.
pub fn export_id(loaded: &Loaded) -> String {
    let mut parts = modules(loaded);
    parts.push(test_fn(loaded));
    parts.join(".")
}
