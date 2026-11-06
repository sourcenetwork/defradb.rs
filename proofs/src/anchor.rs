//! Registry anchors resolved against the tree.
//!
//! An anchor is `;`-separated segments. A segment names paths: a repository
//! path under `crates/`, `tools/` or `proofs/`, or a `name.rs` beside the file
//! named before it. After a path come the symbols it holds, split on `,`,
//! `and`, ` / `, `/` and parentheses: a lone identifier, `Type::item`, or
//! `file.rs::item`. Inside prose only code-shaped words (`::`, `_` or an inner
//! capital) count as symbols. A segment that opens with `repo:` names another
//! repository and is not resolved. Symbols after a directory are not checked.
//!
//! A symbol resolves against what the file declares, together with the
//! module it roots (`name.rs` with `name/`, or any `mod.rs`): a lone name or
//! `file.rs::name` must be declared there, and in `Owner::item` each part must
//! hold the next, as a type holds its fields, variants or `impl` items and a
//! module its items. A name that is only used there does not resolve.

mod declarations;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use declarations::Declarations;

const ROOTS: &[&str] = &["crates/", "tools/", "proofs/"];

/// The repository this crate sits in.
pub fn repository_root() -> PathBuf {
    let proofs = Path::new(env!("CARGO_MANIFEST_DIR"));
    proofs.parent().unwrap_or(proofs).to_path_buf()
}

enum Scope {
    File(PathBuf),
    Dir,
}

/// Every reason `anchor` does not resolve under `root`; empty when it does.
pub fn problems(root: &Path, anchor: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut read = HashMap::new();
    let mut scope: Option<Scope> = None;
    let mut beside: Option<PathBuf> = None;
    for segment in anchor.split(';') {
        if is_foreign(segment) {
            continue;
        }
        for item in items(&expand_braces(segment)) {
            let tokens: Vec<&str> = item
                .split_whitespace()
                .map(trim)
                .filter(|token| !token.is_empty())
                .collect();
            let mut rest = &tokens[..];
            while let Some((&first, tail)) = rest.split_first() {
                let Some((path, symbol)) = split_path(first) else {
                    break;
                };
                match resolve(root, path, beside.as_deref()) {
                    Some(found) if found.is_dir() => {
                        beside = Some(found);
                        scope = Some(Scope::Dir);
                    }
                    Some(found) => {
                        beside = found.parent().map(Path::to_path_buf);
                        scope = Some(Scope::File(found));
                    }
                    None => {
                        problems.push(format!("`{path}` names no file or directory"));
                        scope = None;
                    }
                }
                if let Some(symbol) = symbol {
                    check(&scope, symbol, &mut read, &mut problems);
                }
                rest = tail;
            }
            let symbols: Vec<&str> = match rest {
                [only] if is_symbol(only) => vec![*only],
                _ => rest
                    .iter()
                    .copied()
                    .filter(|token| is_code(token) && is_symbol(token))
                    .collect(),
            };
            for token in symbols {
                for symbol in token.split('/') {
                    check(&scope, symbol, &mut read, &mut problems);
                }
            }
        }
    }
    problems
}

fn is_foreign(segment: &str) -> bool {
    let first = segment.split_whitespace().next().unwrap_or("");
    first.split_once(':').is_some_and(|(repo, rest)| {
        !repo.is_empty()
            && !rest.starts_with(':')
            && repo
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

fn expand_braces(segment: &str) -> String {
    let (Some(open), Some(close)) = (segment.find('{'), segment.find('}')) else {
        return segment.to_string();
    };
    if close < open {
        return segment.to_string();
    }
    let start = segment[..open]
        .rfind(char::is_whitespace)
        .map_or(0, |i| i + 1);
    let end = segment[close..]
        .find(char::is_whitespace)
        .map_or(segment.len(), |i| close + i);
    let (prefix, suffix) = (&segment[start..open], &segment[close + 1..end]);
    let words: Vec<String> = segment[open + 1..close]
        .split(',')
        .map(|stem| format!("{prefix}{stem}{suffix}"))
        .collect();
    let expanded = format!(
        "{}{}{}",
        &segment[..start],
        words.join(" "),
        &segment[end..]
    );
    expand_braces(&expanded)
}

fn items(segment: &str) -> Vec<String> {
    segment
        .replace(['(', ')'], ",")
        .replace(" / ", ",")
        .replace(" and ", ",")
        .split(',')
        .map(str::to_string)
        .collect()
}

fn trim(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, '`' | '\'' | '"' | '.' | ',' | ':'))
}

fn split_path(token: &str) -> Option<(&str, Option<&str>)> {
    if let Some(at) = token.find(".rs::") {
        return Some((&token[..at + 3], Some(&token[at + 5..])));
    }
    let rooted = ROOTS.iter().any(|root| token.starts_with(root));
    (token.ends_with(".rs") || rooted).then_some((token, None))
}

fn resolve(root: &Path, path: &str, beside: Option<&Path>) -> Option<PathBuf> {
    if ROOTS.iter().any(|prefix| path.starts_with(prefix)) {
        let found = root.join(path);
        return found.exists().then_some(found);
    }
    let mut dir = beside?;
    loop {
        let found = dir.join(path);
        if found.exists() {
            return Some(found);
        }
        dir = dir.parent().filter(|parent| parent.starts_with(root))?;
    }
}

fn is_symbol(token: &&str) -> bool {
    !token.is_empty()
        && token.split('/').all(|path| {
            path.split("::").all(|part| {
                part.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
}

fn is_code(token: &&str) -> bool {
    token.contains("::")
        || token.contains('_')
        || token.chars().skip(1).any(|c| c.is_ascii_uppercase())
            && token.chars().any(|c| c.is_ascii_lowercase())
}

fn check(
    scope: &Option<Scope>,
    symbol: &str,
    read: &mut HashMap<PathBuf, Declarations>,
    problems: &mut Vec<String>,
) {
    let Some(Scope::File(file)) = scope else {
        return;
    };
    let found = read
        .entry(file.clone())
        .or_insert_with(|| Declarations::read(&module_text(file)));
    let parts: Vec<&str> = symbol.split("::").collect();
    let missing = match parts.as_slice() {
        [name] => (!found.declares(name)).then(|| format!("no `{name}` declared")),
        _ => parts
            .windows(2)
            .find(|pair| !found.holds(pair[0], pair[1]))
            .map(|pair| format!("`{}` holds no `{}`", pair[0], pair[1])),
    };
    if let Some(missing) = missing {
        problems.push(format!("`{symbol}`: {missing} in {}", file.display()));
    }
}

fn module_text(file: &Path) -> String {
    let mut text = fs::read_to_string(file).unwrap_or_default();
    let module = match file.file_name().and_then(|name| name.to_str()) {
        Some("mod.rs") => file.parent().map(Path::to_path_buf),
        _ => Some(file.with_extension("")),
    };
    if let Some(dir) = module.filter(|dir| dir.is_dir()) {
        append_sources(&dir, &mut text);
    }
    text
}

fn append_sources(dir: &Path, text: &mut String) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            append_sources(&path, text);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            text.push('\n');
            text.push_str(&fs::read_to_string(&path).unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::PROPERTIES;

    #[test]
    fn every_registry_anchor_resolves() {
        let root = repository_root();
        let unresolved: Vec<String> = PROPERTIES
            .iter()
            .flat_map(|property| {
                problems(&root, property.anchor)
                    .into_iter()
                    .map(move |problem| format!("{}: {problem}", property.name))
            })
            .collect();
        assert!(
            unresolved.is_empty(),
            "registry anchors that name nothing in the tree:\n{}",
            unresolved.join("\n")
        );
    }

    #[test]
    fn a_moved_file_or_renamed_symbol_is_refused() {
        let root = repository_root();
        let cases = [
            (
                "crates/p2p/src/sync/no_such_module.rs",
                "`crates/p2p/src/sync/no_such_module.rs`",
            ),
            (
                "crates/p2p/src/sync/queue.rs ProcessQueue::no_such_item",
                "no `no_such_item`",
            ),
            (
                "crates/p2p/src/sync/queue.rs; no_such_sibling.rs",
                "`no_such_sibling.rs`",
            ),
            (
                "crates/p2p/src/sync/queue.rs::no_such_item",
                "no `no_such_item`",
            ),
            (
                "crates/p2p/src/sync/queue.rs try_acquire, no_such_item",
                "no `no_such_item`",
            ),
            (
                "crates/p2p/src/sync/queue.rs the no_such_item path",
                "no `no_such_item`",
            ),
            (
                "crates/p2p/src/sync/{queue,no_such_module}.rs",
                "`crates/p2p/src/sync/no_such_module.rs`",
            ),
            (
                "crates/p2p/src/sync/queue.rs ProcessQueue::cid",
                "`ProcessQueue` holds no `cid`",
            ),
            (
                "crates/db/src/block/heads/document.rs supersede",
                "no `supersede` declared",
            ),
        ];
        for (anchor, expected) in cases {
            let found = problems(&root, anchor);
            assert!(
                found.iter().any(|problem| problem.contains(expected)),
                "{anchor}: expected a problem naming {expected}, got {found:?}"
            );
        }
    }

    #[test]
    fn prose_directories_and_foreign_repositories_are_not_symbols() {
        let root = repository_root();
        for anchor in [
            "crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync",
            "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) push replay",
            "crates/p2p/src/sync/queue.rs read-only admission order",
            "gents:crates/gents/src/lifecycle/claim.rs:claim_inner (foreign substrate)",
        ] {
            assert_eq!(problems(&root, anchor), Vec::<String>::new(), "{anchor}");
        }
    }
}
