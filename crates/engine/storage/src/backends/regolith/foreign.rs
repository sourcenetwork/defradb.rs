//! Refusing a store folder another engine wrote, before regolith touches it.
//!
//! A root directory Go DefraDB created keeps a Badger datastore where this
//! node keeps its regolith store. regolith reads Badger's `MANIFEST` as its
//! own and fails on its first bytes, after it has already made folders of its
//! own beside Badger's files. Looking first answers with what the folder
//! holds and changes nothing in it.

use std::io::Read;
use std::path::Path;

use crate::corekv::{Error, IoFailure, Result};

/// The first bytes of every Badger `MANIFEST` (badger's `magicText`).
const BADGER_MANIFEST_MAGIC: &[u8; 4] = b"Bdgr";

/// The file Badger keeps its data keys in, written at every open; regolith
/// never names a file so.
const BADGER_KEY_REGISTRY: &str = "KEYREGISTRY";

/// Fails with [`Error::GoBadgerStore`] when `path` holds a Badger datastore.
/// A missing folder, or one with neither Badger file, passes.
pub(super) fn refuse_badger(path: &Path) -> Result<()> {
    if path.join(BADGER_KEY_REGISTRY).is_file() || manifest_is_badger(path)? {
        return Err(Error::GoBadgerStore(path.display().to_string()));
    }
    Ok(())
}

fn manifest_is_badger(path: &Path) -> Result<bool> {
    let manifest = path.join("MANIFEST");
    let mut file = match std::fs::File::open(&manifest) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io(error, &manifest)),
    };
    let mut head = [0u8; BADGER_MANIFEST_MAGIC.len()];
    match file.read_exact(&mut head) {
        Ok(()) => Ok(&head == BADGER_MANIFEST_MAGIC),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(io(error, &manifest)),
    }
}

fn io(error: std::io::Error, file: &Path) -> Error {
    Error::Io(IoFailure::from(error).with_context(format!("failed to read '{}'", file.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header Badger v4 writes at the top of a new `MANIFEST`: its magic
    /// text and format version 8, big-endian.
    const BADGER_V4_HEADER: [u8; 8] = *b"Bdgr\0\0\0\x08";

    #[test]
    fn a_badger_manifest_is_refused_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MANIFEST"), BADGER_V4_HEADER).unwrap();
        let error = refuse_badger(dir.path()).unwrap_err();
        assert_eq!(
            error,
            Error::GoBadgerStore(dir.path().display().to_string())
        );
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn opening_a_badger_folder_fails_before_regolith_writes_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MANIFEST"), BADGER_V4_HEADER).unwrap();
        let refused = Error::GoBadgerStore(dir.path().display().to_string());
        assert_eq!(
            crate::RegolithStore::open(dir.path()).err(),
            Some(refused.clone())
        );
        let options = crate::RegolithStoreOptions::default();
        assert_eq!(
            super::super::RegolithTxn::open_read_only(dir.path(), options).err(),
            Some(refused)
        );
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn a_badger_key_registry_alone_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(BADGER_KEY_REGISTRY), [0u8; 96]).unwrap();
        assert!(matches!(
            refuse_badger(dir.path()),
            Err(Error::GoBadgerStore(_))
        ));
    }

    #[test]
    fn the_magic_matches_the_lean_model() {
        let model = include_str!("../../../../../../proofs/lean/ForeignStoreRefusal.lean");
        let line = model
            .lines()
            .find(|line| line.starts_with("def badgerMagic"))
            .expect("the model defines badgerMagic");
        let modeled: Vec<u8> = line
            .split('\'')
            .skip(1)
            .step_by(2)
            .flat_map(|c| c.bytes())
            .collect();
        assert_eq!(modeled, BADGER_MANIFEST_MAGIC);
    }

    #[test]
    fn a_missing_empty_or_regolith_folder_passes() {
        let dir = tempfile::tempdir().unwrap();
        refuse_badger(&dir.path().join("absent")).unwrap();
        refuse_badger(dir.path()).unwrap();
        std::fs::write(dir.path().join("MANIFEST"), b"Bd").unwrap();
        refuse_badger(dir.path()).unwrap();
        let store = crate::RegolithStore::open(dir.path().join("regolith")).unwrap();
        drop(store);
        refuse_badger(&dir.path().join("regolith")).unwrap();
    }
}
