//! What the corpus does not carry, read from the Go test source at the
//! parity pin: each test's `SupportedClientTypes`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

/// Go's client types, as the Rust clients that stand for them.
const CLIENTS: &[(&str, &[&str])] = &[
    ("GoClientType", &["InProcess"]),
    ("HTTPClientType", &["RustHttp", "GoHttp"]),
    ("CLIClientType", &["Cli"]),
    ("CClientType", &[]),
    ("JavaClientType", &[]),
    ("JSClientType", &[]),
];

/// The Go repository at one commit, its files read once.
pub struct GoSource {
    repo: PathBuf,
    pin: String,
    files: BTreeMap<String, Option<String>>,
}

/// A test's client restriction as Go states it.
pub enum Clients {
    /// The test runs under every client.
    All,
    /// The Rust clients that stand for Go's list, in Go's order.
    Only(Vec<&'static str>),
    /// Go names its list in a way this reader does not follow; the reason.
    Unresolved(String),
}

impl GoSource {
    pub fn new(repo: PathBuf, pin: String) -> Self {
        Self {
            repo,
            pin,
            files: BTreeMap::new(),
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    fn file(&mut self, path: &str) -> Option<String> {
        let (repo, pin) = (&self.repo, &self.pin);
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| {
                let out = Command::new("git")
                    .arg("-C")
                    .arg(repo)
                    .args(["show", &format!("{pin}:{path}")])
                    .output()
                    .ok()?;
                out.status
                    .success()
                    .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
            })
            .clone()
    }

    fn package_files(&self, dir: &str) -> Vec<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(["ls-tree", "--name-only", &self.pin, &format!("{dir}/")])
            .output();
        out.map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|name| name.ends_with(".go"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
    }

    /// The `SupportedClientTypes` of Go test `test` in `path`.
    pub fn clients(&mut self, path: &str, test: &str) -> Clients {
        let Some(text) = self.file(path) else {
            return Clients::Unresolved(format!("{path} is not at {}", self.pin));
        };
        let Some(body) = function_body(&text, test) else {
            return Clients::Unresolved(format!("no func {test} in {path}"));
        };
        let mut found: Vec<Vec<&'static str>> = Vec::new();
        for value in body
            .match_indices("SupportedClientTypes:")
            .map(|(at, key)| body[at + key.len()..].trim_start())
        {
            let list = match value.strip_prefix("immutable.Some(") {
                Some(rest) => list(rest),
                None if value.starts_with("immutable.None") => continue,
                None => {
                    let name: String = value
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    match self.package_value(path, &name) {
                        Some(list) => list,
                        None => {
                            return Clients::Unresolved(format!("{path}::{test} names {name:?}"))
                        }
                    }
                }
            };
            if !found.contains(&list) {
                found.push(list);
            }
        }
        match found.len() {
            0 => Clients::All,
            1 => Clients::Only(found.remove(0)),
            _ => Clients::Unresolved(format!("{path}::{test} has {} lists", found.len())),
        }
    }

    /// The list a package-level `name = immutable.Some(...)` holds.
    fn package_value(&mut self, path: &str, name: &str) -> Option<Vec<&'static str>> {
        if name.is_empty() {
            return None;
        }
        let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        for file in self.package_files(&dir) {
            let text = self.file(&file)?;
            for (at, _) in text.match_indices(name) {
                let before = text[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
                let rest = text[at + name.len()..].trim_start();
                if let Some(value) = rest.strip_prefix('=') {
                    if let Some(open) = value.trim_start().strip_prefix("immutable.Some(") {
                        return Some(list(open));
                    }
                }
            }
        }
        None
    }
}

/// Go's `func test(` body, up to the next top-level `func`.
fn function_body<'a>(text: &'a str, test: &str) -> Option<&'a str> {
    let start = text.find(&format!("\nfunc {test}("))? + 1;
    let end = text[start + 1..]
        .find("\nfunc ")
        .map_or(text.len(), |n| start + 1 + n);
    Some(&text[start..end])
}

/// The Rust clients for the `state.XClientType` names inside one
/// `immutable.Some(` argument, `rest` starting just after its `(`.
fn list(rest: &str) -> Vec<&'static str> {
    let mut depth = 1;
    let end = rest
        .char_indices()
        .find(|(_, c)| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            depth == 0
        })
        .map_or(rest.len(), |(at, _)| at);
    let span = &rest[..end];
    let span: String = span
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = Vec::new();
    for (at, _) in span.match_indices("state.") {
        let name: String = span[at + "state.".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        if let Some((_, rust)) = CLIENTS.iter().find(|(go, _)| *go == name) {
            for client in *rust {
                if !out.contains(client) {
                    out.push(*client);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_inline_list_without_comments() {
        let rest = "[]state.ClientType{\n// state.CLIClientType is out\nstate.HTTPClientType,\nstate.JSClientType}),\n";
        assert_eq!(list(rest), vec!["RustHttp", "GoHttp"]);
    }

    #[test]
    fn a_body_ends_at_the_next_func() {
        let text =
            "package x\n\nfunc TestA(t *testing.T) {\n a\n}\n\nfunc TestB(t *testing.T) {\n b\n}\n";
        assert_eq!(
            function_body(text, "TestA"),
            Some("func TestA(t *testing.T) {\n a\n}\n")
        );
    }
}
