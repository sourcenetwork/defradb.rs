//! The one-time rewrite of a store the previous release encrypted value by
//! value.
//!
//! The previous release sealed each value above the engine with a wrapper
//! (see [`wrapped`]). The engine now seals its own files and reads values as
//! plain bytes, so a store the wrapper wrote would open and serve the
//! wrapper's ciphertext as values. Every open of a store on disk therefore
//! looks for that ciphertext first and, finding it, rewrites the store once:
//! each value is decrypted and written plain into a new store the engine
//! seals under the current at-rest key, the copy is read back and compared
//! with what was copied, and only then does it take the old store's place.
//!
//! How a wrapper store is told apart: every store the previous release's
//! database opened holds the index format stamp (`/system/index_format`, four
//! bytes), which the wrapper stored as 32. A store without the stamp holds
//! nothing a database wrote; with keys configured, its first entry is checked
//! anyway.
//!
//! After a power cut at any step the next open finds the old store or the
//! checked copy, never neither and never a mix, and a write made after the
//! rewrite survives. Two programs opening one store take turns through a lock.
//! Memory stays bounded by bytes whatever the store's size ([`copy`]).

use std::path::Path;
use std::sync::Arc;

use regolith::{Db, OptimisticTransactionDb, Options};

use super::config::RegolithStoreOptions;
use super::encryption::AtRestKeys;
use crate::corekv::{Error, Result};
use crate::keys::systemstore::{AT_REST_REWRITE_KEY, INDEX_FORMAT_KEY};
use crate::namespace::Namespace;

mod copy;
mod protocol;
#[cfg(test)]
mod tests;
mod wrapped;

use protocol::{Names, RewriteLock};
use wrapped::WrapperKey;

/// The places a test stops the rewrite, as a power cut would: it panics there,
/// so no cleanup after that point runs. Every other caller does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// One entry was written into the copy.
    Copying,
    /// The copy holds every entry and is closed, not yet checked.
    Copied,
    /// The copy was checked and marked.
    Verified,
    /// The old store was renamed aside; `P` is missing.
    MovedAside,
    /// The copy was renamed to `P`.
    MovedIn,
    /// The old store was deleted.
    Retired,
}

/// Opens the store at `path` with the engine options `engine`, rewriting it
/// first if the previous release's wrapper wrote it.
pub(super) fn open(
    path: &Path,
    engine: Options,
    options: &RegolithStoreOptions,
) -> Result<OptimisticTransactionDb> {
    open_with_steps(path, engine, options, &mut |_| {})
}

fn open_with_steps(
    path: &Path,
    engine: Options,
    options: &RegolithStoreOptions,
    step: &mut dyn FnMut(Step),
) -> Result<OptimisticTransactionDb> {
    let env = options.files_env();
    // A store kept in memory or in a browser was never written by the wrapper,
    // and its folders are not on this file system.
    if !Arc::ptr_eq(&env, &regolith::env::std_env()) {
        return open_engine(path, engine);
    }
    let names = Names::new(path)?;
    let mut lock = None;
    if names.leftovers(&*env) {
        lock = Some(RewriteLock::take(&names, &*env)?);
        names.recover(&*env)?;
    }
    let keys = options.at_rest_keys();
    // With keys, look before opening read-write: such an open seals the
    // store's manifest under the current key, and a wrapper store must keep
    // opening with the key it was written with until its rewrite succeeds.
    // Once sealed, the keyless look stops at the manifest's first bytes.
    if let Some(source) = keys
        .map(|_| look(path, &options.engine))
        .transpose()?
        .flatten()
    {
        if let Some(key) = settle(&source, keys, &names, &*env)? {
            let lock = match lock {
                Some(lock) => lock,
                None => RewriteLock::take(&names, &*env)?,
            };
            rewrite(Source::Look(source), &key, &names, options, &*env, step)?;
            lock.release(&names, &*env)?;
            return open_engine(path, engine);
        }
    }
    let db = open_engine(path, engine.clone())?;
    let Some(key) = settle(db.db(), keys, &names, &*env)? else {
        if let Some(lock) = lock {
            lock.release(&names, &*env)?;
        }
        return Ok(db);
    };
    let lock = match lock {
        Some(lock) => lock,
        None => RewriteLock::take(&names, &*env)?,
    };
    rewrite(Source::Live(db), &key, &names, options, &*env, step)?;
    lock.release(&names, &*env)?;
    open_engine(path, engine)
}

/// A read-only handle on the store at `path`, opened without keys, or `None`
/// when there is no store yet or its files are sealed: only this release
/// seals them, and it never writes wrapper values.
fn look(path: &Path, engine: &Options) -> Result<Option<Db>> {
    match std::fs::read_dir(path) {
        Ok(entries) => {
            if entries.into_iter().next().is_none() {
                return Ok(None);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::Io(
                crate::corekv::IoFailure::from(error).with_context(format!(
                    "failed to read the store folder '{}'",
                    path.display()
                )),
            ))
        }
    }
    match Db::open_read_only(path, engine.clone()) {
        Ok(db) => Ok(Some(db)),
        Err(regolith::Error::KeyProviderRequired) => Ok(None),
        Err(error) => Err(Error::Backend(format!(
            "failed to open regolith read-only: {error}"
        ))),
    }
}

/// An old store left beside `db` is deleted only
/// when `db` is its rewrite, then `db` is checked for wrapper values.
fn settle<'k>(
    db: &Db,
    keys: Option<&'k AtRestKeys>,
    names: &Names,
    env: &dyn regolith::Env,
) -> Result<Option<WrapperKey<'k>>> {
    if names.legacy_exists(env) {
        if db.get(&marker_key()).map_err(read)?.is_none() {
            return Err(Error::Backend(format!(
                "found an earlier store at '{}' beside a store at '{}' that is not its rewrite; move one of them away and open again",
                names.legacy.display(),
                names.store.display()
            )));
        }
        names.retire_legacy(env)?;
    }
    inspect(db, keys)
}

/// The store the rewrite copies from: the keyless read-only look, or the
/// read-write handle when no keys were configured to look with.
enum Source {
    Look(Db),
    Live(OptimisticTransactionDb),
}

impl Source {
    fn db(&self) -> &Db {
        match self {
            Self::Look(db) => db,
            Self::Live(db) => db.db(),
        }
    }

    /// Closes the handle and drops it, so the folder is free to rename.
    fn close(self) -> Result<()> {
        self.db().close().map_err(|error| {
            Error::Backend(format!(
                "failed to close the store before its rewrite: {error}"
            ))
        })
    }
}

/// Refuses a read-only open of a store that still needs the rewrite, or
/// that an interrupted rewrite left folders beside: a read-only open never
/// changes them.
pub(super) fn check_read_only(path: &Path, db: &Db, options: &RegolithStoreOptions) -> Result<()> {
    let env = options.engine.get_env();
    if Arc::ptr_eq(env, &regolith::env::std_env()) && Names::new(path)?.leftovers(&**env) {
        return Err(Error::Backend(
            "an interrupted store rewrite left folders beside this store; open it read-write once to finish it".into(),
        ));
    }
    if inspect(db, options.at_rest_keys())?.is_some() {
        return Err(Error::Backend(
            "this store still holds values an earlier release encrypted; open it read-write once with its at-rest key to rewrite it".into(),
        ));
    }
    Ok(())
}

fn open_engine(path: &Path, engine: Options) -> Result<OptimisticTransactionDb> {
    OptimisticTransactionDb::open(path, engine)
        .map_err(|error| Error::Backend(format!("failed to open regolith: {error}")))
}

fn read(error: regolith::Error) -> Error {
    Error::Backend(format!("failed to read the store at open: {error}"))
}

fn marker_key() -> Vec<u8> {
    Namespace::Systemstore.prefix_key(AT_REST_REWRITE_KEY)
}

/// Whether the wrapper wrote `db`, and the key it used. A wrapper store with
/// no key, or with none of the keys that open it, is refused: serving it
/// would hand out ciphertext as values.
fn inspect<'k>(db: &Db, keys: Option<&'k AtRestKeys>) -> Result<Option<WrapperKey<'k>>> {
    let stamp_key = Namespace::Systemstore.prefix_key(INDEX_FORMAT_KEY);
    match db.get(&stamp_key).map_err(read)? {
        Some(stamp) if stamp.len() == 4 + wrapped::OVERHEAD => {
            let Some(keys) = keys else {
                return Err(Error::Backend(
                    "this store was written by an earlier release with at-rest encryption; open it with at-rest encryption on and the key it was written with, so it can be rewritten once".into(),
                ));
            };
            match WrapperKey::find(keys.all(), &stamp_key, &stamp) {
                Some(key) => Ok(Some(key)),
                None => Err(Error::Backend(
                    "none of the configured at-rest keys decrypts this store; open it with the key an earlier release wrote it with, as the current or an older key".into(),
                )),
            }
        }
        Some(_) => Ok(None),
        None => {
            let Some(keys) = keys else { return Ok(None) };
            match db.iter().into_iter().next() {
                Some(entry) => {
                    let (key, value) = entry.map_err(read)?;
                    Ok(WrapperKey::find(keys.all(), &key, &value))
                }
                None => Ok(None),
            }
        }
    }
}

/// Copy, check, mark, then swap the copy in. The
/// old store is closed before it is renamed, and its folder is deleted only
/// once the copy is durably at `P`.
fn rewrite(
    source: Source,
    key: &WrapperKey<'_>,
    names: &Names,
    options: &RegolithStoreOptions,
    env: &dyn regolith::Env,
    step: &mut dyn FnMut(Step),
) -> Result<()> {
    let options = &options.engine_with_keys();
    names.discard_copy(env)?;
    let copied = copy::copy(source.db(), key, &names.copy, options, step);
    let checked = copied.and_then(|tally| {
        step(Step::Copied);
        copy::verify_and_mark(&names.copy, options, &tally, &marker_key())?;
        Ok(tally)
    });
    let tally = match checked {
        Ok(tally) => tally,
        Err(error) => {
            names.discard_copy(env)?;
            return Err(error);
        }
    };
    step(Step::Verified);
    source.close()?;
    names.move_aside(env)?;
    step(Step::MovedAside);
    names.move_in(env)?;
    step(Step::MovedIn);
    names.retire_legacy(env)?;
    step(Step::Retired);
    tracing::info!(
        entries = tally.entries,
        "rewrote a store an earlier release encrypted value by value"
    );
    Ok(())
}
