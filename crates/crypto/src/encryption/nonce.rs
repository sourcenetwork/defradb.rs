//! Nonce generation for AES-GCM encryption
//!
//! This module provides nonce generation for AES-GCM authenticated encryption.
//! In production, nonces are generated using cryptographically secure random numbers.
//! Deterministic nonces are only for Go-compatibility tests, the way Go switches
//! to `generateTestNonce` inside a test binary. A test harness turns them on for
//! a process and every node it spawns with `DEFRA_DETERMINISTIC_TEST_NONCE=1`, or
//! in-process with [`set_deterministic_nonce`]. Release builds also require
//! `DEFRA_ALLOW_DETERMINISTIC_TEST_CRYPTO=1`; production deployments must never
//! set either variable.

use rand::rngs::OsRng;
use rand::RngCore;
use std::sync::atomic::{AtomicU8, Ordering};

use defra_core::Result;

use crate::types::AES_NONCE_SIZE;

const DETERMINISTIC_TEST_CRYPTO_ENV: &str = "DEFRA_ALLOW_DETERMINISTIC_TEST_CRYPTO";
const DETERMINISTIC_TEST_NONCE_ENV: &str = "DEFRA_DETERMINISTIC_TEST_NONCE";

const MODE_UNSET: u8 = 0;
const MODE_RANDOM: u8 = 1;
const MODE_DETERMINISTIC: u8 = 2;

/// The nonce mode, read from the environment on first use unless
/// [`set_deterministic_nonce`] set it first.
static NONCE_MODE: AtomicU8 = AtomicU8::new(MODE_UNSET);

/// Generate a cryptographically secure random nonce for AES-GCM
///
/// # Returns
/// A 12-byte (96-bit) nonce suitable for AES-GCM encryption
///
/// # Example
/// ```ignore
/// let nonce = generate_nonce()?;
/// assert_eq!(nonce.len(), 12);
/// ```
pub fn generate_nonce() -> Result<[u8; AES_NONCE_SIZE]> {
    if deterministic_nonce_enabled() {
        return generate_deterministic_nonce();
    }

    generate_random_nonce()
}

/// Generate a random nonce using cryptographically secure RNG
fn generate_random_nonce() -> Result<[u8; AES_NONCE_SIZE]> {
    let mut nonce = [0u8; AES_NONCE_SIZE];
    OsRng.try_fill_bytes(&mut nonce).map_err(|e| {
        crate::error::crypto_error(format!("RNG failure in nonce generation: {}", e))
    })?;
    Ok(nonce)
}

/// Generate a deterministic nonce for testing
///
/// **WARNING: This should NEVER be used in production.** It's only for testing
/// purposes to ensure reproducible test results.
///
/// Uses the same deterministic value as the Go implementation:
/// "deterministic nonce for testing" (first 12 bytes)
#[doc(hidden)]
pub fn generate_deterministic_nonce() -> Result<[u8; AES_NONCE_SIZE]> {
    // Match Go's generateTestNonce(): []byte("deterministic nonce for testing")[:12]
    let full_nonce = b"deterministic nonce for testing";
    let mut nonce = [0u8; AES_NONCE_SIZE];
    nonce.copy_from_slice(&full_nonce[..AES_NONCE_SIZE]);
    Ok(nonce)
}

/// Control whether to use deterministic nonces in test/debug builds.
///
/// **WARNING: This should NEVER be set to true in production.**
/// Only use for testing with reproducible nonces. In release builds this
/// function refuses to enable deterministic mode unless
/// `DEFRA_ALLOW_DETERMINISTIC_TEST_CRYPTO=1` is present in the process
/// environment. Overrides `DEFRA_DETERMINISTIC_TEST_NONCE`.
#[doc(hidden)]
pub fn set_deterministic_nonce(enabled: bool) {
    NONCE_MODE.store(mode(enabled), Ordering::Release);
}

#[doc(hidden)]
pub fn deterministic_nonce_enabled() -> bool {
    let current = NONCE_MODE.load(Ordering::Acquire);
    let current = if current == MODE_UNSET {
        let from_env = mode(env_is_one(DETERMINISTIC_TEST_NONCE_ENV));
        // A racing set_deterministic_nonce wins over the environment.
        match NONCE_MODE.compare_exchange(MODE_UNSET, from_env, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => from_env,
            Err(set) => set,
        }
    } else {
        current
    };
    current == MODE_DETERMINISTIC
}

fn mode(deterministic: bool) -> u8 {
    if deterministic && deterministic_test_crypto_allowed() {
        MODE_DETERMINISTIC
    } else {
        MODE_RANDOM
    }
}

fn deterministic_test_crypto_allowed() -> bool {
    cfg!(debug_assertions) || env_is_one(DETERMINISTIC_TEST_CRYPTO_ENV)
}

fn env_is_one(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_nonce_is_gos_test_nonce() {
        assert_eq!(&generate_deterministic_nonce().unwrap(), b"deterministi");
    }

    #[test]
    fn env_mode_is_read_once_and_an_explicit_set_overrides_it() {
        let saved = NONCE_MODE.load(Ordering::Acquire);
        NONCE_MODE.store(MODE_UNSET, Ordering::Release);
        let allowed = deterministic_test_crypto_allowed();
        let expected = env_is_one(DETERMINISTIC_TEST_NONCE_ENV) && allowed;
        assert_eq!(deterministic_nonce_enabled(), expected);
        assert_ne!(NONCE_MODE.load(Ordering::Acquire), MODE_UNSET);

        set_deterministic_nonce(true);
        assert_eq!(deterministic_nonce_enabled(), allowed);
        set_deterministic_nonce(false);
        assert!(!deterministic_nonce_enabled());
        assert_ne!(generate_nonce().unwrap(), generate_nonce().unwrap());

        NONCE_MODE.store(saved, Ordering::Release);
    }
}
