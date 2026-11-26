//! Wiping a closed node's store for a purge.
//!
//! The data directory is emptied (opened, every key dropped, closed), never
//! deleted; the other directories the node owns are removed; the kept paths
//! (the keyring) are never reached. `proofs/lean/StorePurge.lean` proves the
//! removal set against [`check`], and `proofs/tla/StorePurge.tla` the order:
//! the wipe runs only after the node closed, and the node opens only after it.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::corekv::{Error, Result};
use crate::{RegolithStore, RegolithStoreOptions};

/// How many symbolic links one resolution follows before it refuses, as the
/// kernel's `ELOOP` limit does.
const MAX_LINKS: usize = 40;

/// What a purge wipes once the node is closed.
#[derive(Clone, Default)]
pub struct WipeTarget {
    /// The on-disk store and the options it was opened with, not placed on a
    /// pool (the node's pool is stopped by then); `None` for a memory store,
    /// which the close already dropped.
    pub store: Option<(PathBuf, RegolithStoreOptions)>,
    /// Directories the node owns besides the store, removed whole: the
    /// embedded Vera node's chain and journal.
    pub owned_dirs: Vec<PathBuf>,
    /// Paths no removal may equal or sit above: the keyring.
    pub kept: Vec<PathBuf>,
}

impl std::fmt::Debug for WipeTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WipeTarget")
            .field("store", &self.store.as_ref().map(|(path, _)| path))
            .field("owned_dirs", &self.owned_dirs)
            .field("kept", &self.kept)
            .finish()
    }
}

/// Refuses a target that would remove the data directory, a kept path or a
/// directory above either, before anything is touched. Paths are compared as
/// the filesystem resolves them: symbolic links followed, `.` and `..`
/// resolved, so `root/../root/vera/keys` and `root/vera/link/../keys` are seen
/// where they lead. A kept path is also refused when a symbolic link it passes
/// through sits in an owned directory, such as `root/vera/keys` linking to a
/// keyring outside `root/vera`: removing the directory removes the link, and
/// the configured path no longer leads to the keyring. An absent directory
/// still passes; a path that cannot be resolved is refused.
pub fn check(target: &WipeTarget) -> Result<()> {
    let owned = target
        .owned_dirs
        .iter()
        .map(|dir| Ok((dir, resolve_removal(dir)?)))
        .collect::<Result<Vec<_>>>()?;
    let protected = target
        .kept
        .iter()
        .chain(target.store.as_ref().map(|(path, _)| path));
    for keep in protected {
        let resolved = resolve(keep)?;
        let reached: Vec<&PathBuf> = std::iter::once(&resolved.path)
            .chain(&resolved.links)
            .collect();
        if let Some((dir, _)) = owned
            .iter()
            .find(|(_, dir)| reached.iter().any(|place| place.starts_with(dir)))
        {
            return Err(Error::Backend(format!(
                "refusing to wipe '{}': it holds '{}', which a purge keeps",
                dir.display(),
                keep.display()
            )));
        }
    }
    Ok(())
}

/// Where a path leads, and the symbolic links it passes through on the way.
struct Resolved {
    /// The path with every link followed and every `.` and `..` resolved.
    path: PathBuf,
    /// Each link followed, as the place its own entry sits (its resolved
    /// parent joined with its name): removing a directory above that entry
    /// breaks the path even when the link points elsewhere.
    links: Vec<PathBuf>,
}

/// Where removing `dir` reaches. `remove_dir_all` removes a symbolic link
/// itself, never what it points to, so a link in the last component is kept
/// as a name under its resolved parent.
fn resolve_removal(dir: &Path) -> Result<PathBuf> {
    let is_link = std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_symlink());
    match (is_link, dir.parent(), dir.file_name()) {
        (true, Some(parent), Some(name)) => Ok(resolve(parent)?.path.join(name)),
        _ => Ok(resolve(dir)?.path),
    }
}

/// `path` as the filesystem reads it now, always absolute, walked one
/// component at a time. A symbolic link is recorded and replaced by its
/// target's components; `.` is dropped; `..` takes back the component before
/// it, which is never a link because every link met so far was replaced. A
/// component that does not exist is kept as a name, so an absent directory
/// resolves too. A relative path starts at the working directory.
fn resolve(path: &Path) -> Result<Resolved> {
    let refuse = |reason: String| {
        Error::Backend(format!(
            "refusing to wipe: cannot resolve '{}': {reason}",
            path.display()
        ))
    };
    let absolute = std::path::absolute(path).map_err(|error| refuse(error.to_string()))?;
    let mut pending: VecDeque<OsString> = absolute
        .components()
        .map(|part| part.as_os_str().to_os_string())
        .collect();
    let mut resolved = Resolved {
        path: PathBuf::new(),
        links: Vec::new(),
    };
    while let Some(part) = pending.pop_front() {
        match Path::new(&part).components().next() {
            Some(Component::Normal(name)) => {
                let entry = resolved.path.join(name);
                match std::fs::symlink_metadata(&entry) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        if resolved.links.len() == MAX_LINKS {
                            return Err(refuse("too many levels of symbolic links".into()));
                        }
                        let target = std::fs::read_link(&entry)
                            .map_err(|error| refuse(error.to_string()))?;
                        if target.is_absolute() {
                            resolved.path = PathBuf::new();
                        }
                        for part in target.components().rev() {
                            pending.push_front(part.as_os_str().to_os_string());
                        }
                        resolved.links.push(entry);
                    }
                    Ok(_) => resolved.path = entry,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                        ) =>
                    {
                        resolved.path = entry;
                    }
                    Err(error) => return Err(refuse(error.to_string())),
                }
            }
            Some(Component::ParentDir) => {
                if matches!(
                    resolved.path.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    resolved.path.pop();
                }
            }
            Some(Component::CurDir) | None => {}
            Some(root) => resolved.path.push(root),
        }
    }
    Ok(resolved)
}

/// Wipes `target`: drops every key of the store on disk, then removes the
/// owned directories. Blocking; run it off the data path
/// (`Edge::spawn_blocking`) once the node has closed.
pub fn wipe(target: &WipeTarget) -> Result<()> {
    check(target)?;
    if let Some((path, options)) = &target.store {
        RegolithStore::open_with_options(path, options.clone())?.drop_all_and_close()?;
    }
    for dir in &target.owned_dirs {
        remove_dir(dir)?;
    }
    Ok(())
}

fn remove_dir(dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Error::Backend(format!(
            "failed to remove '{}': {error}",
            dir.display()
        ))),
        _ => Ok(()),
    }
}
