//! `POST /purge` under `defradb start`: Go's `PurgeAndRestart`.
//!
//! The request records the purge and asks the node to shut down; `run`
//! returns [`RunOutcome::Purged`] only when the Vera node stopped and the
//! database closed; `execute` wipes the store off the data path and starts
//! the node again on the same address. `proofs/tla/StorePurge.tla` models the
//! order, `proofs/lean/StorePurge.lean` the removal set [`wipe_target`] builds.

use std::sync::atomic::{AtomicBool, Ordering};

use nuclei::sync::mpsc;

use super::node::Node;
use crate::config::{Config, DatastoreType};
use crate::error::{Error, Result};

/// How a run of the node ended.
#[derive(Debug)]
pub enum RunOutcome {
    /// A signal or a crashed server stopped it.
    Stopped,
    /// A purge stopped it cleanly: wipe this, then start it again.
    Purged(Box<storage::WipeTarget>),
}

/// How a run ends once its teardown is over: a purge whose Vera stop or
/// database close failed ends `defradb start` with that error and wipes
/// nothing (the CliChecksVeraStop and CliChecksClose guards of
/// `proofs/tla/StorePurge.tla`).
pub(super) fn outcome(
    purge_requested: bool,
    teardown_failure: Option<String>,
    target: &storage::WipeTarget,
) -> Result<RunOutcome> {
    match (purge_requested, teardown_failure) {
        (false, _) => Ok(RunOutcome::Stopped),
        (true, Some(reason)) => Err(Error::PurgeTeardown(reason)),
        (true, None) => Ok(RunOutcome::Purged(Box::new(target.clone()))),
    }
}

/// Records the first purge and asks the node to shut down.
pub(super) struct CliPurge {
    requested: AtomicBool,
    shutdown: mpsc::Sender<()>,
    development: bool,
}

impl CliPurge {
    pub(super) fn new(shutdown: mpsc::Sender<()>, development: bool) -> Self {
        Self {
            requested: AtomicBool::new(false),
            shutdown,
            development,
        }
    }

    pub(super) fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

impl defra_http::PurgeOperations for CliPurge {
    fn request_purge(&self) -> std::result::Result<(), defra_http::PurgeRefusal> {
        if !self.requested.swap(true, Ordering::SeqCst) {
            // A full channel already holds a shutdown, which this one joins.
            let _ = self.shutdown.try_send(());
        }
        Ok(())
    }

    fn development(&self) -> bool {
        self.development
    }
}

/// What a purge wipes: the store on disk (none in memory), the embedded Vera
/// node's chain and journal whatever the store, and never the keyring.
pub(super) fn wipe_target(config: &Config) -> Result<storage::WipeTarget> {
    let store = match config.datastore.store {
        DatastoreType::Memory => None,
        DatastoreType::Regolith => Some((
            config.data_path(),
            Node::encryption_options(config, super::node::regolith_options(config), false)?,
        )),
    };
    let owned_dirs = match config.acp.document_type {
        #[cfg(feature = "embedded-vera")]
        crate::config::AcpDocumentType::VeraRsEmbedded => {
            vec![
                config.rootdir.join("vera"),
                config.rootdir.join("vera-worker"),
            ]
        }
        _ => Vec::new(),
    };
    let target = storage::WipeTarget {
        store,
        owned_dirs,
        kept: vec![config.keyring_path()],
    };
    storage::check_wipe(&target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> storage::WipeTarget {
        storage::WipeTarget {
            store: None,
            owned_dirs: vec!["/rootdir/vera".into()],
            kept: Vec::new(),
        }
    }

    #[test]
    fn a_failed_teardown_ends_the_purge_with_its_error_and_nothing_to_wipe() {
        for reason in [
            "the embedded Vera node did not stop: deadline",
            "the database did not close: busy",
        ] {
            match outcome(true, Some(reason.into()), &target()) {
                Err(Error::PurgeTeardown(got)) => assert_eq!(got, reason),
                other => panic!("a failed teardown must end the run, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_clean_teardown_hands_over_the_target_and_a_stop_hands_over_none() {
        assert!(matches!(
            outcome(true, None, &target()),
            Ok(RunOutcome::Purged(got)) if got.owned_dirs == target().owned_dirs
        ));
        assert!(matches!(
            outcome(false, Some("busy".into()), &target()),
            Ok(RunOutcome::Stopped)
        ));
    }
}
