//! A C host's keyring for the node's peer key, given to the node through the
//! `peer_keyring_` fields of `NodeInitOptions`: a platform keystore
//! (Keychain, Android Keystore) the host owns and secures, adapted to
//! [`keyring::Keyring`].
//!
//! The node keeps the peer key there instead of in plaintext in its store.
//! A keyring error stops the node rather than falling back to the store, and
//! a store that already holds a plaintext peer key is refused.

use std::ffi::{c_char, c_int, CString};
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::types::NodeInitOptions;

/// What a keyring callback returns when it has no key by that name.
pub const DEFRA_KEYRING_NOT_FOUND: c_int = 1;

/// The longest key a host keyring may hold, in bytes.
pub const DEFRA_KEYRING_MAX_KEY_LEN: usize = 4096;

/// Reads the key stored under `name`.
///
/// Write the key into `out_key` when it fits in `out_key_capacity` bytes, set
/// `out_key_len` to its length either way, and return `0`. Return
/// `DEFRA_KEYRING_NOT_FOUND` only when no key has that name, and any other
/// value when the keystore failed or is locked: the node then does not start.
pub type DefraKeyringGetCallback = unsafe extern "C" fn(
    context: usize,
    name: *const c_char,
    out_key: *mut u8,
    out_key_capacity: usize,
    out_key_len: *mut usize,
) -> c_int;

/// Stores the `key_len` bytes at `key` under `name`, replacing any key
/// stored there. Return `0` once stored, any other value on failure.
pub type DefraKeyringSetCallback = unsafe extern "C" fn(
    context: usize,
    name: *const c_char,
    key: *const u8,
    key_len: usize,
) -> c_int;

struct HostKeyring {
    get: DefraKeyringGetCallback,
    set: DefraKeyringSetCallback,
    context: usize,
}

fn key_name(name: &str) -> keyring::Result<CString> {
    keyring::KeyName::validate(name)?;
    CString::new(name).map_err(|_| keyring::Error::InvalidKeyName(name.to_owned()))
}

fn failed(operation: &str, name: &str, status: c_int) -> keyring::Error {
    keyring::Error::SystemKeyring(format!(
        "host keyring {} of '{}' failed with status {}",
        operation, name, status
    ))
}

impl keyring::Keyring for HostKeyring {
    fn set(&self, name: &str, key: &[u8]) -> keyring::Result<()> {
        let c_name = key_name(name)?;
        // SAFETY: the host promised a thread-safe callback when it set it,
        // and the name and key live until it returns.
        match unsafe { (self.set)(self.context, c_name.as_ptr(), key.as_ptr(), key.len()) } {
            0 => Ok(()),
            status => Err(failed("set", name, status)),
        }
    }

    fn get(&self, name: &str) -> keyring::Result<Zeroizing<Vec<u8>>> {
        let c_name = key_name(name)?;
        let mut buffer = Zeroizing::new(vec![0u8; DEFRA_KEYRING_MAX_KEY_LEN]);
        let mut len = 0usize;
        // SAFETY: as for `set`; the buffer holds `DEFRA_KEYRING_MAX_KEY_LEN`
        // bytes and `len` outlives the call.
        let status = unsafe {
            (self.get)(
                self.context,
                c_name.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut len,
            )
        };
        match status {
            0 if len > DEFRA_KEYRING_MAX_KEY_LEN => Err(keyring::Error::SystemKeyring(format!(
                "host keyring key '{}' is {} bytes, more than the {} a key may hold",
                name, len, DEFRA_KEYRING_MAX_KEY_LEN
            ))),
            0 => Ok(Zeroizing::new(buffer[..len].to_vec())),
            DEFRA_KEYRING_NOT_FOUND => Err(keyring::Error::NotFound(name.to_owned())),
            status => Err(failed("get", name, status)),
        }
    }

    fn delete(&self, name: &str) -> keyring::Result<()> {
        Err(keyring::Error::SystemKeyring(format!(
            "host keyring delete of '{name}' is not supported"
        )))
    }

    fn list(&self) -> keyring::Result<Vec<String>> {
        Err(keyring::Error::SystemKeyringListNotSupported)
    }
}

/// The host keyring `options` sets, if any: its two callbacks are set
/// together or not at all.
pub(crate) fn from_options(
    options: &NodeInitOptions,
) -> Result<Option<defra_node::PeerKeyring>, String> {
    match (options.peer_keyring_get, options.peer_keyring_set) {
        (Some(get), Some(set)) => Ok(Some(defra_node::PeerKeyring::new(Arc::new(HostKeyring {
            get,
            set,
            context: options.peer_keyring_context,
        })))),
        (None, None) => Ok(None),
        _ => Err("peer_keyring_get and peer_keyring_set must be set together".to_string()),
    }
}

#[cfg(test)]
#[path = "peer_keyring_tests.rs"]
mod tests;
