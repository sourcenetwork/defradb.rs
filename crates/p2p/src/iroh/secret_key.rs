/// Generate an ephemeral iroh secret key.
pub fn generate_secret_key() -> iroh::SecretKey {
    iroh::SecretKey::generate()
}

/// Adopt an externally held 32-byte iroh secret key.
///
/// The browser has no filesystem to persist an endpoint key to, so it supplies
/// one from whatever store it already trusts with the node identity.
pub fn secret_key_from_bytes(bytes: [u8; 32]) -> iroh::SecretKey {
    iroh::SecretKey::from_bytes(&bytes)
}

/// Load an iroh secret key from disk, or generate and persist a new one.
///
/// - If `path` is `Some` and the file exists, reads the 32-byte key from it.
/// - If `path` is `Some` and the file does not exist, generates a new key,
///   creates parent directories as needed, writes the key, and restricts
///   file permissions to owner-only (0o600 on Unix).
/// - If `path` is `None`, generates an ephemeral key (not persisted).
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub async fn load_or_generate_secret_key(
    path: Option<&std::path::Path>,
) -> crate::Result<iroh::SecretKey> {
    use crate::Error;

    let fail = |what: &str, path: &std::path::Path| {
        let what = what.to_owned();
        let path = path.display().to_string();
        move |e: std::io::Error| Error::SecretKey(format!("{what} '{path}': {e}"))
    };

    match path {
        Some(path) if path.exists() => {
            let bytes = nuclei::fs::read(path)
                .await
                .map_err(fail("failed to read iroh secret key", path))?;
            let array: [u8; 32] = bytes.try_into().map_err(|_| {
                Error::SecretKey("iroh secret key file must contain exactly 32 bytes".into())
            })?;
            Ok(secret_key_from_bytes(array))
        }
        Some(path) => {
            let key = generate_secret_key();
            if let Some(parent) = path.parent() {
                nuclei::fs::create_dir_all(parent)
                    .await
                    .map_err(fail("failed to create iroh key directory", parent))?;
            }
            nuclei::fs::write(path, key.to_bytes())
                .await
                .map_err(fail("failed to write iroh secret key", path))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                nuclei::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .await
                    .map_err(fail("failed to set permissions on", path))?;
            }
            Ok(key)
        }
        None => Ok(generate_secret_key()),
    }
}
