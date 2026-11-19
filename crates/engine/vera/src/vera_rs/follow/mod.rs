//! Following a deployment across a switch of its active device.
//!
//! An account's Vera deployment runs on every device, one active at a time.
//! When another device takes over without a handover, the blocks the
//! replaced device finalized above the new term's cut are void, although they
//! verify under the deployment's consensus key. The light client, built with
//! the deployment's genesis identifier, takes the new term only under its
//! certificate and drops the revision it voids; the height it follows then
//! goes down.
//!
//! What the node decided or confirmed from the void blocks rests on state the
//! deployment does not have. [`Follow`] keeps the provider from deciding or
//! sending anything while a certified term is unchecked
//! ([`Follow::admit`]), and [`reconcile`] checks it: the journal's
//! submissions the deployment no longer holds are sent again exactly as
//! signed and in order, and what a replay does not restore is owed again
//! through the outbox. Every decision is cached under [`Follow::epoch`], the
//! term last checked, so nothing cached before a term is served after it. A
//! term that may have voided more than the journal keeps fails closed for
//! good, with the reason (`proofs/tla/FollowDeployment.tla`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use alloy_primitives::B256;
use defra_core::thread_bounds::MaybeBoxFuture;

use crate::provider::ProviderError;

mod chain;
pub(crate) mod effect;
#[cfg(test)]
mod fixtures;
pub(crate) mod journal;
mod lost;
mod owed;

pub(crate) use chain::ProviderChain;
pub(crate) use journal::{Confirmation, Journal, JournalError};
pub(crate) use owed::owed_again;

use effect::effect_of;
use journal::Floor;
use lost::{lost_effects, tainted_by, Losses};

/// The deployment, as the provider's reconcile needs to reach it.
pub(crate) trait Chain {
    /// Whether the deployment holds `submission`: its receipt is served at a
    /// revision the newest certified term admits.
    fn holds(&self, submission: B256) -> MaybeBoxFuture<'_, Result<bool, ProviderError>>;

    /// Sends the signed request `wire` again and waits for its receipt:
    /// whether it executed successfully, and the height it was confirmed at.
    fn replay<'a>(&'a self, wire: &'a [u8]) -> MaybeBoxFuture<'a, Result<Replayed, ProviderError>>;
}

/// The receipt of a replayed request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Replayed {
    pub executed: bool,
    pub height: u64,
}

/// Where the provider stands with the deployment's terms. Read on every
/// decision without waiting.
#[derive(Debug, Default)]
pub(crate) struct Follow {
    handled: AtomicU64,
    undelivered: AtomicBool,
    closed: regolith::sync::OnceCell<String>,
}

impl Follow {
    /// The state the journal records.
    pub(crate) fn from_journal(journal: &Journal) -> Self {
        let follow = Self::default();
        follow.update(journal);
        follow
    }

    /// Takes what `journal` records now.
    pub(crate) fn update(&self, journal: &Journal) {
        self.handled
            .fetch_max(journal.handled_term(), Ordering::AcqRel);
        self.undelivered
            .store(!journal.undelivered().is_empty(), Ordering::Release);
        if let Some(reason) = journal.closed() {
            let _ = self.closed.set(reason.to_owned());
        }
    }

    /// Whether the node could not tell what a term voided.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.get().is_some()
    }

    /// Whether changes a term voided are still to be owed again.
    pub(crate) fn has_undelivered(&self) -> bool {
        self.undelivered.load(Ordering::Acquire)
    }

    /// The term last checked: part of every cached decision's key.
    pub(crate) fn epoch(&self) -> u64 {
        self.handled.load(Ordering::Acquire)
    }

    /// Whether the provider may decide and send while `certified` is the
    /// newest term its light client holds a certificate of. It may not while
    /// that term is unchecked, while changes it voided are still to be owed
    /// again, or once the node could not tell what a term voided.
    pub(crate) fn admit(&self, certified: u64) -> Result<(), ProviderError> {
        if let Some(reason) = self.closed.get() {
            return Err(ProviderError::Unavailable(reason.clone()));
        }
        if certified > self.epoch() || self.undelivered.load(Ordering::Acquire) {
            return Err(ProviderError::Unavailable(format!(
                "the Vera deployment switched its active device (term {certified}); \
                 submissions confirmed on blocks the switch voided are being checked again"
            )));
        }
        Ok(())
    }
}

/// What checking a term came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reconciled {
    /// The term is checked; `owed` (perhaps empty) is to be owed again.
    Checked { owed: Vec<effect::Lost> },
    /// The node cannot tell what the term voided, for the reason given. It
    /// is recorded: the node stays closed across restarts.
    Closed(String),
}

/// Checks term `certified` against `journal` through `chain`: sends again
/// what the term voided and records what is to be owed again. An error
/// leaves the term unchecked, with what was already learnt recorded, to be
/// checked again.
pub(crate) async fn reconcile<C>(
    journal: &mut Journal,
    chain: &C,
    certified: u64,
) -> Result<Reconciled, ProviderError>
where
    C: Chain,
{
    if let Some(reason) = journal.closed() {
        return Ok(Reconciled::Closed(reason.to_owned()));
    }
    if certified <= journal.handled_term() {
        return Ok(Reconciled::Checked {
            owed: journal.undelivered().to_vec(),
        });
    }
    let sequences: Vec<u64> = journal.sequences().collect();
    let mut missing = Vec::new();
    let mut reached_held = false;
    for sequence in sequences.into_iter().rev() {
        let entry = journal.entry(sequence).map_err(failure)?;
        if chain.holds(entry.submission).await? {
            reached_held = true;
            break;
        }
        missing.push(entry);
    }
    missing.reverse();
    if !reached_held {
        if let Some(reason) = uncovered(journal.floor(), chain).await? {
            journal.close(reason.clone()).map_err(failure)?;
            return Ok(Reconciled::Closed(reason));
        }
    }
    let mut losses = Losses {
        owed: journal.undelivered().to_vec(),
        ..Losses::default()
    };
    let mut tainted = tainted_by(&losses.owed);
    for entry in missing {
        let replayed = chain.replay(&entry.wire).await?;
        let recorded = losses.owed.len();
        lost_effects(
            &mut losses,
            &mut tainted,
            effect_of(&entry.wire),
            replayed.executed,
        );
        if let Some(object) = losses.archives.first() {
            let reason = format!(
                "the Vera deployment switched its active device and voided the archive of \
                 {}/{}/{} this node confirmed; it was refused when sent again, so access it \
                 removed may be back. Archive the object again, then restart the node with a \
                 new worker directory",
                object.0, object.1, object.2
            );
            journal.close(reason.clone()).map_err(failure)?;
            return Ok(Reconciled::Closed(reason));
        }
        // What a replay leaves to be owed is recorded before the next replay,
        // so a check stopped half way still owes it, and still taints its
        // object, when it is run again.
        if losses.owed.len() > recorded {
            journal
                .handled(journal.handled_term(), losses.owed.clone())
                .map_err(failure)?;
        }
        journal
            .record(&Confirmation {
                height: replayed.height,
                ..entry
            })
            .map_err(failure)?;
    }
    if losses.other > 0 {
        tracing::warn!(
            refused = losses.other,
            "requests a Vera term voided were refused when sent again; a policy they created \
             is gone and the documents under it cannot be read until it is created again"
        );
    }
    journal
        .handled(certified, losses.owed.clone())
        .map_err(failure)?;
    Ok(Reconciled::Checked { owed: losses.owed })
}

/// Why the journal cannot vouch for what lies below its oldest entry, when
/// the term may have voided that too.
async fn uncovered<C>(floor: Option<&Floor>, chain: &C) -> Result<Option<String>, ProviderError>
where
    C: Chain,
{
    Ok(match floor {
        None => None,
        Some(Floor::Pruned {
            sequence,
            submission,
        }) => (!chain.holds(*submission).await?).then(|| {
            format!(
                "the Vera deployment switched its active device and voided submissions this \
                 node confirmed at or below sequence {sequence}, older than the {} it keeps; \
                 registrations and relationship changes among them may be lost. Re-register \
                 and re-share the documents changed then, and restart the node with a new \
                 worker directory",
                journal::MAX_CONFIRMATIONS
            )
        }),
        Some(Floor::Unjournaled { before }) => Some(format!(
            "the Vera deployment switched its active device and may have voided submissions \
             this node signed before it kept a journal of them (sequences below {before}); \
             registrations and relationship changes among them may be lost. Re-register and \
             re-share the documents changed then, and restart the node with a new worker \
             directory"
        )),
    })
}

fn failure(error: JournalError) -> ProviderError {
    ProviderError::Unavailable(error.to_string())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
