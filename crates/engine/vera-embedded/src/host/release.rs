//! Whether a Vera node thread still holds the node directory.
//!
//! Every embedded Vera node locks `.node-hold` in its data directory from
//! before it touches its state until its runtime has stopped, and releases
//! it on its own thread just before that thread ends (`directory.rs` and
//! `EmbeddedNode::launch` in vera.rs `crates/vera-node/src/embedded`). A
//! stop that joined the node's thread finds the lock free. A thread nobody
//! joins still holds it: one a switch was halting or launching when a stop
//! or the start deadline dropped the switch, as Vera's `restart` gives the
//! host no handle to such a thread. The stop waits for the lock instead
//! (proofs/tla/EmbeddedVeraSwitch.tla, WaitsRelease).

use std::fs::{File, TryLockError};
use std::io::ErrorKind;
use std::path::Path;

use edge::Edge;

use super::DIRECTORY_RETRY;

/// The lock file of Vera's directory claim.
const CLAIM: &str = ".node-hold";

/// Waits until no Vera node thread holds `directory`. The caller bounds
/// the wait.
pub(super) async fn released(directory: &Path, edge: &Edge) -> std::io::Result<()> {
    while held(directory)? {
        edge.sleep(DIRECTORY_RETRY).await;
    }
    Ok(())
}

/// Whether a Vera node thread holds `directory` now. A directory without
/// the lock file was never claimed. The probe takes the lock only for as
/// long as it looks, and nothing starts a node there while the host stops.
fn held(directory: &Path) -> std::io::Result<bool> {
    let claim = match File::options().write(true).open(directory.join(CLAIM)) {
        Ok(claim) => claim,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    match claim.try_lock() {
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locked_claim_is_held_until_it_is_released() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!held(directory.path()).unwrap(), "never claimed");
        let claim = File::options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.path().join(CLAIM))
            .unwrap();
        claim.try_lock().unwrap();
        assert!(held(directory.path()).unwrap());
        drop(claim);
        assert!(!held(directory.path()).unwrap());
    }
}
