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
///   creates parent directories as needed, writes the seed, and restricts
///   file permissions to owner-only (0o600 on Unix).
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
            nuclei::fs::write(path, seed.as_slice())
                .await
                .map_err(fail("failed to write peer key", path))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                nuclei::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .await
                    .map_err(fail("failed to set permissions on", path))?;
            }
            Ok(seed)
        }
        None => generate_seed(),
    }
}
