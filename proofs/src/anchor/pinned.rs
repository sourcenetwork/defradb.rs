//! The source of a dependency at the version `Cargo.lock` pins.
//!
//! A model that cites another crate's code names it `crate:path`, with `path`
//! relative to that crate's root (`regolith:src/sync/once_cell.rs`). The path
//! is resolved in the source cargo unpacked for the locked version: a
//! registry crate under `$CARGO_HOME/registry/src/*/{name}-{version}`, a git
//! crate in the checkout of its locked commit.

use std::fs;
use std::path::{Path, PathBuf};

/// A package `Cargo.lock` pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub name: String,
    pub version: String,
    /// `None` for a workspace member.
    pub source: Option<String>,
}

/// Every package in a `Cargo.lock`.
pub fn locked(lock: &str) -> Vec<Locked> {
    lock.split("[[package]]")
        .skip(1)
        .filter_map(|block| {
            let field = |key: &str| {
                block.lines().find_map(|line| {
                    let value = line.strip_prefix(key)?.trim_start().strip_prefix('=')?;
                    Some(value.trim().trim_matches('"').to_string())
                })
            };
            Some(Locked {
                name: field("name")?,
                version: field("version")?,
                source: field("source"),
            })
        })
        .collect()
}

/// The cargo home: `$CARGO_HOME`, else `~/.cargo`.
pub fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".cargo")))
}

/// The root of `package`'s unpacked source under `home`, if cargo fetched it.
pub fn source_root(home: &Path, package: &Locked) -> Option<PathBuf> {
    let source = package.source.as_deref()?;
    if source.starts_with("registry+") {
        let unpacked = format!("{}-{}", package.name, package.version);
        return subdirs(&home.join("registry/src"))
            .map(|index| index.join(&unpacked))
            .find(|root| root.join("Cargo.toml").is_file());
    }
    let commit = source.strip_prefix("git+")?.rsplit_once('#')?.1;
    let short = commit.get(..7)?;
    subdirs(&home.join("git/checkouts"))
        .map(|repository| repository.join(short))
        .filter(|checkout| checkout.is_dir())
        .find_map(|checkout| package_dir(&checkout, &package.name, 3))
}

fn subdirs(dir: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
}

fn package_dir(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if declares_package(&dir.join("Cargo.toml"), name) {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    subdirs(dir)
        .filter(|sub| {
            !sub.file_name()
                .is_some_and(|base| base == "target" || base.to_string_lossy().starts_with('.'))
        })
        .find_map(|sub| package_dir(&sub, name, depth - 1))
}

fn declares_package(manifest: &Path, name: &str) -> bool {
    let Ok(text) = fs::read_to_string(manifest) else {
        return false;
    };
    let wanted = format!("\"{name}\"");
    text.split("\n[")
        .find(|section| section.trim_start_matches('[').starts_with("package]"))
        .is_some_and(|package| {
            package.lines().any(|line| {
                line.strip_prefix("name")
                    .and_then(|rest| rest.trim_start().strip_prefix('='))
                    .is_some_and(|value| value.trim() == wanted)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = r#"
[[package]]
name = "db"
version = "0.1.0"

[[package]]
name = "regolith"
version = "0.2.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "00"

[[package]]
name = "acp-light-client"
version = "0.1.0"
source = "git+https://github.com/sourcenetwork/backbone.git?rev=25da44c8#25da44c835b810e854caa938d5c55052da8525e7"
"#;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("conformance-pinned-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_lock_lists_each_package_with_its_source() {
        let packages = locked(LOCK);
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].source, None);
        assert_eq!(packages[1].name, "regolith");
        assert_eq!(packages[1].version, "0.2.5");
        assert!(packages[2]
            .source
            .as_deref()
            .unwrap()
            .ends_with("#25da44c835b810e854caa938d5c55052da8525e7"));
    }

    #[test]
    fn registry_and_git_sources_resolve_to_the_locked_version() {
        let home = scratch("resolve");
        write(
            &home.join("registry/src/index-1/regolith-0.2.4/Cargo.toml"),
            "",
        );
        write(
            &home.join("registry/src/index-1/regolith-0.2.5/Cargo.toml"),
            "",
        );
        write(
            &home.join("git/checkouts/backbone-1/25da44c/crates/acp-light-client/Cargo.toml"),
            "[package]\nname = \"acp-light-client\"\n",
        );
        write(
            &home.join("git/checkouts/backbone-1/0000000/crates/acp-light-client/Cargo.toml"),
            "[package]\nname = \"acp-light-client\"\n",
        );
        let packages = locked(LOCK);
        assert_eq!(
            source_root(&home, &packages[1]),
            Some(home.join("registry/src/index-1/regolith-0.2.5"))
        );
        assert_eq!(
            source_root(&home, &packages[2]),
            Some(home.join("git/checkouts/backbone-1/25da44c/crates/acp-light-client"))
        );
        assert_eq!(source_root(&home, &packages[0]), None);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn an_unfetched_version_has_no_source() {
        let home = scratch("unfetched");
        write(
            &home.join("registry/src/index-1/regolith-0.2.1/Cargo.toml"),
            "",
        );
        assert_eq!(source_root(&home, &locked(LOCK)[1]), None);
        fs::remove_dir_all(&home).unwrap();
    }
}
