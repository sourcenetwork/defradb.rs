//! The folders the rewrite uses beside the store, the lock that lets one
//! program at a time change them, and the renames and deletes that change
//! them.
//!
//! Every rename and delete is followed by a sync of the parent folder before
//! anything depends on it. Without it a power cut can forget the rename that
//! put the copy at `P` after the old store was deleted, or after a write to
//! the copy was acknowledged.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use regolith::env::FileLock;
use regolith::Env;

use crate::corekv::{Error, Result};

/// Where the rewrite works, all beside the store so a rename never crosses a
/// file system: `P` the store, `P.rewrite` the copy being built,
/// `P.legacy` the old store moved aside, `P.rewrite-lock` the lock.
pub(super) struct Names {
    pub(super) store: PathBuf,
    pub(super) copy: PathBuf,
    pub(super) legacy: PathBuf,
    pub(super) lock: PathBuf,
    parent: PathBuf,
}

impl Names {
    pub(super) fn new(store: &Path) -> Result<Self> {
        let name = store.file_name().ok_or_else(|| {
            Error::Backend(format!(
                "store path '{}' does not name a folder",
                store.display()
            ))
        })?;
        let parent = match store.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let beside = |suffix: &str| {
            let mut sibling = OsString::from(name);
            sibling.push(suffix);
            parent.join(sibling)
        };
        Ok(Self {
            store: store.to_path_buf(),
            copy: beside(".rewrite"),
            legacy: beside(".legacy"),
            lock: beside(".rewrite-lock"),
            parent,
        })
    }

    /// A copy, an old store or a lock folder left by an earlier attempt: the
    /// open repairs, under the lock, before it reads anything.
    pub(super) fn leftovers(&self, env: &dyn Env) -> bool {
        [&self.copy, &self.legacy, &self.lock]
            .into_iter()
            .any(|path| env.exists(path))
    }

    pub(super) fn legacy_exists(&self, env: &dyn Env) -> bool {
        env.exists(&self.legacy)
    }

    /// Repairs what an interrupted rewrite left. The old store goes back to `P` when `P` is missing (a power cut between the two
    /// renames); a copy is deleted whenever `P` exists, since a copy only ever
    /// becomes the store by being renamed to `P`; a copy with neither `P` nor
    /// an old store is never trusted on its own.
    pub(super) fn recover(&self, env: &dyn Env) -> Result<()> {
        if env.exists(&self.legacy) && !env.exists(&self.store) {
            self.rename(env, &self.legacy, &self.store)?;
        }
        if env.exists(&self.copy) {
            if !env.exists(&self.store) {
                return Err(Error::Backend(format!(
                    "found an unfinished rewrite at '{}' but no store at '{}'; restore the store and open it again",
                    self.copy.display(),
                    self.store.display()
                )));
            }
            self.remove(env, &self.copy)?;
        }
        Ok(())
    }

    /// Deletes the old store left beside a store the rewrite produced.
    /// The caller has checked that `P` carries the rewrite marker.
    pub(super) fn retire_legacy(&self, env: &dyn Env) -> Result<()> {
        self.remove(env, &self.legacy)
    }

    /// Deletes a copy left by an earlier attempt, or one that failed its check.
    pub(super) fn discard_copy(&self, env: &dyn Env) -> Result<()> {
        if env.exists(&self.copy) {
            self.remove(env, &self.copy)?;
        }
        Ok(())
    }

    /// Renames `P` to `P.legacy`. `P` is missing until [`Self::move_in`].
    pub(super) fn move_aside(&self, env: &dyn Env) -> Result<()> {
        self.rename(env, &self.store, &self.legacy)
    }

    /// Renames `P.rewrite` to `P`.
    pub(super) fn move_in(&self, env: &dyn Env) -> Result<()> {
        self.rename(env, &self.copy, &self.store)
    }

    fn rename(&self, env: &dyn Env, from: &Path, to: &Path) -> Result<()> {
        env.rename(from, to).map_err(|error| {
            io_error(
                error,
                format!("rename '{}' to '{}'", from.display(), to.display()),
            )
        })?;
        self.sync(env)
    }

    /// Deletes a whole folder. Its files go first, so a power cut can leave
    /// it half deleted: the protocol never reads such a folder again.
    fn remove(&self, env: &dyn Env, path: &Path) -> Result<()> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error, format!("remove '{}'", path.display()))),
        }
        self.sync(env)
    }

    fn sync(&self, env: &dyn Env) -> Result<()> {
        env.sync_dir(&self.parent)
            .map_err(|error| io_error(error, format!("sync '{}'", self.parent.display())))
    }
}

fn io_error(error: io::Error, action: String) -> Error {
    Error::Io(
        crate::corekv::IoFailure::from(error)
            .with_context(format!("store rewrite could not {action}")),
    )
}

/// The exclusive lock one program holds while it repairs or rewrites the
/// store's folders: without it, a second program could start its copy in
/// the folder the first is about to rename into place. It never waits: a busy
/// lock fails the open, which a program retries once the other is done.
pub(super) struct RewriteLock {
    folder: PathBuf,
    _guard: Box<dyn FileLock>,
}

impl RewriteLock {
    pub(super) fn take(names: &Names, env: &dyn Env) -> Result<Self> {
        env.create_dir_all(&names.lock)
            .map_err(|error| io_error(error, format!("create '{}'", names.lock.display())))?;
        let guard = env.lock_file(&names.lock, true).map_err(|_| {
            Error::Backend(format!(
                "another program is rewriting or repairing the store at '{}'; open it again once that program is done",
                names.store.display()
            ))
        })?;
        Ok(Self {
            folder: names.lock.clone(),
            _guard: guard,
        })
    }

    /// Give the lock back and delete its folder. A program that fails before
    /// this keeps the folder, and the next one takes the lock in it.
    pub(super) fn release(self, names: &Names, env: &dyn Env) -> Result<()> {
        names.remove(env, &self.folder)
    }
}
