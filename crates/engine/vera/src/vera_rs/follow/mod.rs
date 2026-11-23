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

use std::sync::Arc;

use alloy_primitives::B256;
use defra_core::thread_bounds::MaybeBoxFuture;
use regolith::sync::Atom;

use crate::provider::ProviderError;

mod chain;
pub(crate) mod effect;
#[cfg(test)]
mod fixtures;
pub(crate) mod journal;
mod lost;
pub(crate) mod owed;

pub(crate) use chain::ProviderChain;
pub(crate) use journal::{Confirmation, Journal, JournalError};
pub(crate) use owed::owed_again;

use effect::effect_of;
use journal::Floor;
use lost::{held_again, lost_effects, tainted_by, Losses};
pub(crate) mod report;

pub(crate) use report::TakeoverReport;

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
    /// Refused because what it creates is already there.
    pub exists: bool,
    pub height: u64,
}

/// Where the provider stands with the deployment's terms: the term last
/// checked, whether changes it voided still wait to be owed again, and why
/// the node failed closed, if it did. Published whole, so a reader that sees
/// a term as checked also sees what still blocks admission with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Standing {
    handled: u64,
    undelivered: bool,
    closed: Option<Arc<str>>,
}

/// [`Standing`], read on every decision without waiting.
pub(crate) struct Follow {
    standing: Atom<Standing>,
}

impl Default for Follow {
    fn default() -> Self {
        Self {
            standing: Atom::new(Standing::default()),
        }
    }
}

impl Follow {
    /// The state the journal records.
    pub(crate) fn from_journal(journal: &Journal) -> Self {
        let follow = Self::default();
        follow.update(journal);
        follow
    }

    /// Takes what `journal` records now, in one publication. Only the holder
    /// of the worker, which owns the journal, calls it.
    pub(crate) fn update(&self, journal: &Journal) {
        let previous = self.standing.load_clone();
        self.standing.store(Standing {
            handled: previous.handled.max(journal.handled_term()),
            undelivered: !journal.undelivered().is_empty(),
            closed: journal.closed().map(Arc::from).or(previous.closed),
        });
    }

    /// Whether the node could not tell what a term voided.
    pub(crate) fn is_closed(&self) -> bool {
        self.standing.load_clone().closed.is_some()
    }

    /// Whether changes a term voided are still to be owed again.
    pub(crate) fn has_undelivered(&self) -> bool {
        self.standing.load_clone().undelivered
    }

    /// The term last checked: part of every cached decision's key.
    pub(crate) fn epoch(&self) -> u64 {
        self.standing.load_clone().handled
    }

    /// Whether the provider may decide and send while `certified` is the
    /// newest term its light client holds a certificate of, and if so the
    /// term last checked. It may not while that term is unchecked, while
    /// changes it voided are still to be owed again, or once the node could
    /// not tell what a term voided.
    pub(crate) fn admit(&self, certified: u64) -> Result<u64, ProviderError> {
        let standing = self.standing.load_clone();
        if let Some(reason) = &standing.closed {
            return Err(ProviderError::Unavailable(reason.to_string()));
        }
        if certified > standing.handled || standing.undelivered {
            return Err(unchecked(certified));
        }
        Ok(standing.handled)
    }

    /// Whether what was admitted at `epoch` still is: admitted now, and no
    /// term checked since. A read that a term check overtook was answered at
    /// a state the check may have found void.
    pub(crate) fn still(&self, epoch: u64, certified: u64) -> Result<(), ProviderError> {
        if self.admit(certified)? != epoch {
            return Err(unchecked(certified));
        }
        Ok(())
    }
}

fn unchecked(certified: u64) -> ProviderError {
    ProviderError::Unavailable(format!(
        "the Vera deployment switched its active device (term {certified}); \
         submissions confirmed on blocks the switch voided are being checked again"
    ))
}

/// What checking a term came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reconciled {
    /// The term is checked; the report says what each replay came to.
    Checked { report: TakeoverReport },
    /// The node cannot tell what the term voided, for the reason given. It
    /// is recorded: the node stays closed across restarts. Its partial
    /// report says what the check had learnt when it stopped.
    Closed {
        reason: String,
        report: TakeoverReport,
    },
}

/// Whether a check of `certified` that finishes now is the one that checks
/// it: the journal is open and has not handled the term. Only that check is
/// reported to the embedder; a later one only delivers what the term left
/// owed (`proofs/tla/TakeoverReport.tla`).
pub(crate) fn first_check(journal: &Journal, certified: u64) -> bool {
    journal.closed().is_none() && certified > journal.handled_term()
}

/// Checks term `certified` against `journal` through `chain`: sends again
/// what the term voided and records what is to be owed again. `revise` is
/// told what each request sent again came to, before it is recorded. An error
/// leaves the term unchecked, with what was already learnt recorded, to be
/// checked again.
pub(crate) async fn reconcile<C, R>(
    journal: &mut Journal,
    chain: &C,
    certified: u64,
    revise: &mut R,
) -> Result<Reconciled, ProviderError>
where
    C: Chain,
    R: FnMut(B256, bool) -> Result<(), ProviderError>,
{
    if let Some(reason) = journal.closed() {
        return Ok(Reconciled::Closed {
            reason: reason.to_owned(),
            report: TakeoverReport {
                owed: journal.undelivered().to_vec(),
                ..TakeoverReport::default()
            },
        });
    }
    if certified <= journal.handled_term() {
        return Ok(Reconciled::Checked {
            report: TakeoverReport {
                owed: journal.undelivered().to_vec(),
                ..TakeoverReport::default()
            },
        });
    }
    let unfinished = journal.replaying();
    let from = match unfinished {
        // This term's replay was cut short: it is run again from the same
        // entry, whatever the deployment holds now.
        Some(replaying) if replaying.term == certified => Some(replaying.from),
        _ => {
            let scanned = match scan(journal, chain).await? {
                Ok(first) => first,
                Err(reason) => {
                    journal.close(reason.clone()).map_err(failure)?;
                    return Ok(Reconciled::Closed {
                        reason,
                        report: TakeoverReport::default(),
                    });
                }
            };
            // An earlier term's replay was cut short: its entries are sent
            // again too, whether or not this term kept them.
            match (scanned, unfinished) {
                (Some(first), Some(replaying)) => Some(first.min(replaying.from)),
                (first, None) => first,
                (None, Some(replaying)) => Some(replaying.from),
            }
        }
    };
    let mut losses = Losses {
        owed: journal.undelivered().to_vec(),
        ..Losses::default()
    };
    if let Some(from) = from {
        // Marked before the first replay: a refused replay is held at once,
        // and from then on only this mark says its effect was lost.
        journal.begin_replay(certified, from).map_err(failure)?;
        let mut tainted = tainted_by(&losses.owed);
        let resent: Vec<u64> = journal
            .sequences()
            .filter(|sequence| *sequence >= from)
            .collect();
        for sequence in resent {
            let entry = journal.entry(sequence).map_err(failure)?;
            let replayed = chain.replay(&entry.wire).await?;
            // Whatever else names this request learns what it came to on
            // the history the deployment has now.
            revise(entry.submission, replayed.executed)?;
            let effect = effect_of(&entry.wire);
            let held = held_again(&effect, replayed);
            lost_effects(&mut losses, &mut tainted, effect, held);
            if let Some(crate::vera_rs::follow::report::Unrestorable::Archive {
                policy_id,
                resource,
                object,
                ..
            }) = losses.unrestorable.last()
            {
                let reason = format!(
                    "the Vera deployment switched its active device and voided the archive of \
                     {}/{}/{} this node confirmed; it was refused when sent again, so access it \
                     removed may be back. Archive the object again, then restart the node with \
                     a new worker directory",
                    policy_id, resource, object
                );
                journal.close(reason.clone()).map_err(failure)?;
                return Ok(Reconciled::Closed {
                    reason,
                    report: TakeoverReport {
                        applied: losses.applied,
                        owed: losses.owed,
                        unrestorable: losses.unrestorable,
                    },
                });
            }
            journal
                .record(&Confirmation {
                    height: replayed.height,
                    ..entry
                })
                .map_err(failure)?;
        }
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
    Ok(Reconciled::Checked {
        report: TakeoverReport {
            applied: losses.applied,
            owed: losses.owed,
            unrestorable: losses.unrestorable,
        },
    })
}

/// The first journaled sequence the deployment no longer holds, asking
/// newest first and stopping at the first one held (`None` when it holds the
/// newest), or why the journal cannot vouch for what the term voided when it
/// holds none of them.
async fn scan<C>(journal: &Journal, chain: &C) -> Result<Result<Option<u64>, String>, ProviderError>
where
    C: Chain,
{
    let mut first_missing = None;
    for sequence in journal.sequences().rev().collect::<Vec<_>>() {
        let entry = journal.entry(sequence).map_err(failure)?;
        if chain.holds(entry.submission).await? {
            return Ok(Ok(first_missing));
        }
        first_missing = Some(sequence);
    }
    Ok(match uncovered(journal.floor(), chain).await? {
        Some(reason) => Err(reason),
        None => Ok(first_missing),
    })
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
