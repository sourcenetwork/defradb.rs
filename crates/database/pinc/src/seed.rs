//! The node's peer key file: the 32-byte Ed25519 seed its peer id comes from.

use std::path::Path;

use p2p::{Error, Result};
use zeroize::Zeroizing;

/// A fresh Ed25519 seed.
pub fn generate_seed() -> Result<Zeroizing<[u8; 32]>> {
    let key = crypto::generate_ed25519()
        .map_err(|error| Error::SecretKey(format!("failed to generate a peer key: {error}")))?;
    Ok(Zeroizing::new(key.underlying().to_bytes()))
}

/// Load the seed in the file at `path`, or generate one and persist it there.
///
/// - If `path` is `Some` and the file exists, reads the 32-byte seed from it.
/// - If `path` is `Some` and the file does not exist, generates a new seed,
///   creates parent directories as needed, and writes it to `<path>.tmp`,
///   created new and owner-only (0o600 on Unix) before any byte lands,
///   then synced and renamed over `path`, so a crash never leaves a short or
///   world-readable key file.
/// - If `path` is `None`, generates an ephemeral seed (not persisted).
pub async fn load_or_generate_seed(path: Option<&Path>) -> Result<Zeroizing<[u8; 32]>> {
    let fail = |what: &str, path: &Path| {
        let what = what.to_owned();
        let path = path.display().to_string();
        move |e: std::io::Error| Error::SecretKey(format!("{what} '{path}': {e}"))
    };

    match path {
        Some(path) if path.exists() => {
            let bytes = Zeroizing::new(
                nuclei::fs::read(path)
                    .await
                    .map_err(fail("failed to read peer key", path))?,
            );
            let seed: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                Error::SecretKey("peer key file must contain exactly 32 bytes".into())
            })?;
            Ok(Zeroizing::new(seed))
        }
        Some(path) => {
            let seed = generate_seed()?;
            if let Some(parent) = path.parent() {
                nuclei::fs::create_dir_all(parent)
                    .await
                    .map_err(fail("failed to create peer key directory", parent))?;
            }
            let mut tmp = path.as_os_str().to_owned();
            tmp.push(".tmp");
            let tmp = std::path::PathBuf::from(tmp);
            // A crash between create and rename leaves the temp file; it
            // holds no key a node ever used, so it is dropped.
            match nuclei::fs::remove_file(&tmp).await {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(fail("failed to remove stale peer key", &tmp)(e));
                }
                _ => {}
            }
            let mut options = nuclei::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let file = options
                .open(&tmp)
                .await
                .map_err(fail("failed to create peer key", &tmp))?;
            let (written, mut buf) = file.write_owned(seed.to_vec()).await;
            zeroize::Zeroize::zeroize(&mut buf);
            match written.map_err(fail("failed to write peer key", &tmp))? {
                32 => {}
                n => {
                    return Err(Error::SecretKey(format!(
                        "short write of peer key '{}': {n} of 32 bytes",
                        tmp.display()
                    )))
                }
            }
            file.sync_all()
                .await
                .map_err(fail("failed to sync peer key", &tmp))?;
            drop(file);
            nuclei::fs::rename(&tmp, path)
                .await
                .map_err(fail("failed to write peer key", path))?;
            Ok(seed)
        }
        None => generate_seed(),
    }
}

#[cfg(test)]
mod tests {
    use super::load_or_generate_seed;

    #[test]
    #[mantra_macros::req_test("defra.cli.hub")]
    fn a_new_key_file_is_owner_only_and_reloads_the_same_seed() {
        let dir = std::env::temp_dir().join(format!("pinc-seed-{}", std::process::id()));
        let path = dir.join("nested").join("peer.key");
        let _ = std::fs::remove_dir_all(&dir);
        let reload = path.clone();
        let (first, again) = workers::block_on(edge::testing::edge().spawn(async move {
            let first = load_or_generate_seed(Some(&reload)).await.unwrap();
            let again = load_or_generate_seed(Some(&reload)).await.unwrap();
            (first, again)
        }))
        .expect("the test thread waits")
        .expect("the test body runs to its end");
        assert_eq!(*first, *again);
        assert_eq!(std::fs::read(&path).unwrap(), first.as_slice());
        assert!(!dir.join("nested").join("peer.key.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
