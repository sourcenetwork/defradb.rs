//! Source files named in prose: the models, the proofs crate, `SURVEY.md`, and
//! backticked names in the crates' and tools' docs.
//!
//! A bare `name.rs` or `dir/name.rs` is a file of this repository: some
//! tracked file must end with that path. A file of a dependency is named
//! `crate:path` (`regolith:src/sync/once_cell.rs`) and must exist in the
//! source of the version `Cargo.lock` pins. A file of a repository that is no
//! dependency is named `repo:path`, and only the repositories in [`FOREIGN`]
//! may be named so: their files cannot be checked.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::pinned::{self, Locked};

/// Repositories that are not dependencies, which models may still cite.
pub const FOREIGN: &[(&str, &str)] = &[(
    "gents",
    "source-inc/gents, the agent runtime whose claim protocol proofs/tla/Claim_DESIGN.md and DESIGN.md describe",
)];

/// A source file named in prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// The crate or repository before `:`; `None` for this repository.
    pub repo: Option<String>,
    pub path: String,
}

/// Every source file `text` names, with the line it is on.
pub fn mentions(text: &str) -> Vec<(usize, Named)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(".rs").map(|i| from + i) {
        from = at + 3;
        if bytes.get(at + 3).is_some_and(|&b| is_word(b)) {
            continue;
        }
        let start = after_last(&text[..at], |c| {
            !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
        });
        let raw = &text[start..at + 3];
        if raw.starts_with("//") {
            continue;
        }
        // `../a.rs`, `./a.rs`, `/.../a.rs` and `-a.rs` name the path after them.
        let path = raw.trim_start_matches(['.', '/', '-']);
        let stem = path.rsplit('/').next().unwrap_or(path);
        let Some(stem) = stem.strip_suffix(".rs") else {
            continue;
        };
        if stem.is_empty()
            || !stem
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            continue;
        }
        let line = text[..at].matches('\n').count() + 1;
        found.push((
            line,
            Named {
                repo: repo_before(text, start),
                path: path.to_string(),
            },
        ));
    }
    found
}

/// The source files `text` names inside backticks, each span exactly one name,
/// optionally followed by a line or line range.
pub fn backticked(text: &str) -> Vec<(usize, Named)> {
    let mut found = Vec::new();
    let mut offset = 0;
    for (i, span) in text.split('`').enumerate() {
        if i % 2 == 1 && !span.contains(char::is_whitespace) {
            let line = text[..offset].matches('\n').count() + 1;
            found.extend(
                mentions(span)
                    .into_iter()
                    .filter(|(_, named)| whole(span, named))
                    .map(|(_, named)| (line, named)),
            );
        }
        offset += span.len() + 1;
    }
    found
}

fn whole(span: &str, named: &Named) -> bool {
    let expected = match &named.repo {
        Some(repo) => format!("{repo}:{}", named.path),
        None => named.path.clone(),
    };
    span.strip_prefix(&expected)
        .is_some_and(|suffix| suffix.is_empty() || line_suffix(suffix))
}

fn line_suffix(suffix: &str) -> bool {
    let Some(lines) = suffix.strip_prefix(':') else {
        return false;
    };
    let mut lines = lines.split('-');
    let Some(start) = lines.next() else {
        return false;
    };
    let number = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    number(start) && lines.next().is_none_or(number) && lines.next().is_none()
}

/// The byte index just past the last char of `text` that `stop` matches.
fn after_last<F>(text: &str, stop: F) -> usize
where
    F: Fn(char) -> bool,
{
    text.char_indices()
        .rev()
        .find(|&(_, c)| stop(c))
        .map_or(0, |(i, c)| i + c.len_utf8())
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn repo_before(text: &str, start: usize) -> Option<String> {
    let head = text[..start].strip_suffix(':')?;
    let name_start = after_last(head, |c| {
        !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
    let name = &head[name_start..];
    let before = head[..name_start].chars().next_back();
    let free =
        before.is_none_or(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '/' | '.')));
    (!name.is_empty() && free).then(|| name.to_string())
}

/// What a name is checked against.
pub struct Tree {
    suffixes: BTreeSet<String>,
    baseline: BTreeSet<String>,
    locked: Vec<Locked>,
    home: Option<PathBuf>,
}

impl Tree {
    /// `tracked` are repository paths; `baseline` the bare words that look
    /// like file names but are not.
    pub fn new<'a, T, B>(
        tracked: T,
        baseline: B,
        locked: Vec<Locked>,
        home: Option<PathBuf>,
    ) -> Self
    where
        T: IntoIterator<Item = &'a str>,
        B: IntoIterator<Item = &'a str>,
    {
        let mut suffixes = BTreeSet::new();
        for path in tracked {
            suffixes.insert(path.to_string());
            for (i, _) in path.match_indices('/') {
                suffixes.insert(path[i + 1..].to_string());
            }
        }
        Self {
            suffixes,
            baseline: baseline.into_iter().map(str::to_string).collect(),
            locked,
            home,
        }
    }

    /// Why `named` does not resolve, or `None` when it does.
    pub fn problem(&self, named: &Named) -> Option<String> {
        match &named.repo {
            None => self.local(&named.path),
            Some(repo) => self.external(repo, &named.path),
        }
    }

    fn local(&self, path: &str) -> Option<String> {
        let base = path.rsplit('/').next().unwrap_or(path);
        if self.suffixes.contains(path) || self.baseline.contains(base) {
            return None;
        }
        let module = format!("{}/mod.rs", path.trim_end_matches(".rs"));
        if self.suffixes.contains(&module) {
            return Some(format!("`{path}` is now the directory module `{module}`"));
        }
        Some(format!("`{path}` names no tracked file"))
    }

    fn external(&self, repo: &str, path: &str) -> Option<String> {
        let packages: Vec<&Locked> = self.locked.iter().filter(|p| p.name == repo).collect();
        if packages.is_empty() {
            return (!FOREIGN.iter().any(|(name, _)| *name == repo))
                .then(|| format!("`{repo}:{path}`: `{repo}` is neither in Cargo.lock nor a listed foreign repository"));
        }
        let Some(home) = &self.home else {
            return Some(format!(
                "`{repo}:{path}`: no cargo home to find `{repo}`'s source in"
            ));
        };
        packages.iter().find_map(|package| {
            if package.source.is_none() {
                return Some(format!(
                    "`{repo}:{path}`: `{repo}` is a workspace crate; name its repository path"
                ));
            }
            let found = pinned::source_root(home, package);
            let at = format!("{repo} {}", package.version);
            match found {
                None => Some(format!(
                    "`{repo}:{path}`: the source of {at} is not fetched; run `cargo fetch`"
                )),
                Some(root) if !Path::new(&root).join(path).is_file() => {
                    Some(format!("`{repo}:{path}` names no file in {at}"))
                }
                Some(_) => None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;
    use crate::anchor::repository_root;

    fn named(repo: Option<&str>, path: &str) -> Named {
        Named {
            repo: repo.map(str::to_string),
            path: path.to_string(),
        }
    }

    fn found(text: &str) -> Vec<Named> {
        mentions(text).into_iter().map(|(_, named)| named).collect()
    }

    #[test]
    fn mentions_read_bare_names_paths_and_dependency_paths() {
        assert_eq!(
            found("drain.rs:12 and crates/db/src/a.rs::f, then regolith:src/sync/once_cell.rs:78."),
            vec![
                named(None, "drain.rs"),
                named(None, "crates/db/src/a.rs"),
                named(Some("regolith"), "src/sync/once_cell.rs"),
            ]
        );
        assert_eq!(
            found("(acp-light-client:src/term.rs certify)"),
            vec![named(Some("acp-light-client"), "src/term.rs")]
        );
    }

    #[test]
    fn mentions_skip_urls_rust_paths_and_braces() {
        assert_eq!(found("https://github.com/x/y/blob/main/src/a.rs"), vec![]);
        assert_eq!(found("db::merge::x.rs"), vec![named(None, "x.rs")]);
        assert_eq!(found("one—a.rs"), vec![named(None, "a.rs")]);
        assert_eq!(
            found("../block/a.rs and /.../b.rs"),
            vec![named(None, "block/a.rs"), named(None, "b.rs")]
        );
        assert_eq!(found("crates/x/{a,b}.rs and a.rsx and Foo.rs"), vec![]);
    }

    #[test]
    fn backticks_count_only_a_whole_name() {
        let text = "one `a.rs`, then\n`regolith:src/b.rs:78` and `see c.rs`\n\
                    `d.rs:526-530` and `e.rs:` and `f.rs::1`";
        let names: Vec<(usize, Named)> = backticked(text);
        assert_eq!(
            names,
            vec![
                (1, named(None, "a.rs")),
                (2, named(Some("regolith"), "src/b.rs")),
                (3, named(None, "d.rs"))
            ]
        );
    }

    fn tree(locked: Vec<Locked>, home: Option<PathBuf>) -> Tree {
        Tree::new(
            [
                "crates/db/src/index/deferred/drain.rs",
                "crates/p2p/src/sync/pending_store/mod.rs",
            ],
            ["defradb.rs"],
            locked,
            home,
        )
    }

    #[test]
    fn a_local_name_resolves_by_its_trailing_path() {
        let tree = tree(vec![], None);
        for path in [
            "drain.rs",
            "deferred/drain.rs",
            "crates/db/src/index/deferred/drain.rs",
            "defradb.rs",
        ] {
            assert_eq!(tree.problem(&named(None, path)), None, "{path}");
        }
        assert!(tree
            .problem(&named(None, "manager/drain.rs"))
            .unwrap()
            .contains("names no tracked file"));
        assert!(tree
            .problem(&named(None, "replace.rs"))
            .unwrap()
            .contains("names no tracked file"));
    }

    #[test]
    fn a_module_moved_into_a_directory_is_named_by_its_mod_rs() {
        let tree = tree(vec![], None);
        let problem = tree.problem(&named(None, "sync/pending_store.rs")).unwrap();
        assert!(
            problem.contains("directory module `sync/pending_store/mod.rs`"),
            "{problem}"
        );
        assert_eq!(tree.problem(&named(None, "pending_store/mod.rs")), None);
    }

    #[test]
    fn an_external_name_resolves_in_the_pinned_source_only() {
        let home = std::env::temp_dir().join(format!("conformance-prose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let root = home.join("registry/src/index-1/regolith-0.2.5");
        fs::create_dir_all(root.join("src/sync")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();
        fs::write(root.join("src/sync/once_cell.rs"), "").unwrap();
        let old = home.join("registry/src/index-1/regolith-0.2.1/src");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("atom.rs"), "").unwrap();
        let locked = pinned::locked(
            "[[package]]\nname = \"regolith\"\nversion = \"0.2.5\"\nsource = \"registry+x\"\n\
             [[package]]\nname = \"db\"\nversion = \"0.1.0\"\n\
             [[package]]\nname = \"nuclei\"\nversion = \"0.5.9\"\nsource = \"registry+x\"\n",
        );
        let tree = tree(locked, Some(home.clone()));
        assert_eq!(
            tree.problem(&named(Some("regolith"), "src/sync/once_cell.rs")),
            None
        );
        let cases = [
            (
                named(Some("regolith"), "src/atom.rs"),
                "names no file in regolith 0.2.5",
            ),
            (
                named(Some("nuclei"), "src/coop.rs"),
                "the source of nuclei 0.5.9 is not fetched",
            ),
            (named(Some("db"), "src/lib.rs"), "is a workspace crate"),
            (
                named(Some("kovan"), "src/atom.rs"),
                "neither in Cargo.lock nor a listed foreign",
            ),
        ];
        for (name, expected) in cases {
            let problem = tree.problem(&name).unwrap_or_default();
            assert!(problem.contains(expected), "{name:?}: {problem}");
        }
        assert_eq!(
            tree.problem(&named(Some("gents"), "crates/gents/src/watcher.rs")),
            None
        );
        fs::remove_dir_all(&home).unwrap();
    }

    /// The checker's own fixtures name files that do not exist on purpose.
    fn is_fixture(path: &str) -> bool {
        path == "proofs/src/anchor.rs" || path.starts_with("proofs/src/anchor/")
    }

    #[test]
    fn every_source_file_named_in_prose_resolves() {
        let root = repository_root();
        let listing = Command::new("git")
            .args(["ls-files", "-z"])
            .current_dir(&root)
            .output()
            .expect("git ls-files runs in the repository");
        let listing = String::from_utf8(listing.stdout).expect("tracked paths are UTF-8");
        let tracked: Vec<&str> = listing
            .split('\0')
            .filter(|path| !path.is_empty())
            .collect();
        let baseline =
            fs::read_to_string(root.join("proofs/anchor-baseline.txt")).unwrap_or_default();
        let words = baseline
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'));
        let lock = fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock is tracked");
        let tree = Tree::new(
            tracked.iter().copied(),
            words,
            pinned::locked(&lock),
            pinned::cargo_home(),
        );
        let mut problems = Vec::new();
        for path in &tracked {
            let prose = path.starts_with("proofs/") || *path == "SURVEY.md";
            let docs = path.starts_with("crates/") || path.starts_with("tools/");
            if !(prose || docs) || is_fixture(path) {
                continue;
            }
            let Ok(text) = fs::read_to_string(root.join(path)) else {
                continue;
            };
            let names = if prose {
                mentions(&text)
            } else {
                backticked(&text)
            };
            for (line, name) in names {
                if let Some(problem) = tree.problem(&name) {
                    problems.push(format!("{path}:{line}: {problem}"));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "source files named in prose that resolve to nothing (retarget them; name a dependency's \
             file `crate:path`):\n{}",
            problems.join("\n")
        );
    }
}
