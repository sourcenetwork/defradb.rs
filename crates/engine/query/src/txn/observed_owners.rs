//! The provider owner answers one transaction was told, kept for its commit.
//!
//! A document ACP that cannot join the transaction (vera.rs) answers an owner
//! lookup at the edge, as a prepared input. Nothing in storage sees that
//! answer change, so the transaction keeps every one it used here, and its
//! commit reads them all again from the provider first: the commit runs only
//! when each is confirmed as exactly itself (`QueryRunner::commit_txn_as`).
//! Modeled by proofs/lean/RemoteOwnerAnswer.lean (`verdict`) and
//! proofs/tla/RemoteOwnerObservation.tla.

use std::sync::Arc;

use regolith::sync::Atom;

use crate::prepared::PreparedInputRequest;

/// One provider answer: the exact request and the provider's exact response.
pub type OwnerAnswer = (PreparedInputRequest, Arc<[u8]>);

/// A provider owner answer the transaction used is not the provider's answer
/// at commit: the owner changed (a transfer or an archive finalized on the
/// provider) or the confirmation did not read it. Nothing was committed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("owner changed: a document owner this transaction was told is no longer the provider's")]
pub struct OwnerChanged;

/// The provider owner answers of one transaction, in the order it used them.
/// Requests of one transaction run one at a time, so an answer is appended as
/// a whole new snapshot and readers never wait.
pub struct ObservedOwners {
    answers: Atom<Arc<Vec<OwnerAnswer>>>,
}

impl Default for ObservedOwners {
    fn default() -> Self {
        Self {
            answers: Atom::new(Arc::new(Vec::new())),
        }
    }
}

impl std::fmt::Debug for ObservedOwners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservedOwners")
            .field("answers", &self.answers.load().len())
            .finish()
    }
}

impl ObservedOwners {
    /// Keep an answer the transaction used. Every answer is kept, a repeated
    /// request included: two different answers to one request can never
    /// both be confirmed.
    pub fn record(&self, request: PreparedInputRequest, answer: Arc<[u8]>) {
        self.answers.rcu(|old| {
            let mut next = Vec::with_capacity(old.len() + 1);
            next.extend(old.iter().cloned());
            next.push((request.clone(), answer.clone()));
            Arc::new(next)
        });
    }

    /// Each distinct request the transaction used, once, for the
    /// confirmation round.
    pub fn requests(&self) -> Vec<PreparedInputRequest> {
        let answers = self.answers.load_clone();
        let mut seen = rapidhash::fast::RapidHashSet::default();
        answers
            .iter()
            .filter(|(request, _)| seen.insert(request.clone()))
            .map(|(request, _)| request.clone())
            .collect()
    }

    /// Whether every answer kept is confirmed as exactly itself by
    /// `confirmations`, the provider's answers read again at commit.
    pub fn confirm(&self, confirmations: &[OwnerAnswer]) -> Result<(), OwnerChanged> {
        let answers = self.answers.load_clone();
        let holds = |(request, answer): &OwnerAnswer| {
            confirmations
                .iter()
                .find(|(confirmed, _)| confirmed == request)
                .is_some_and(|(_, confirmed)| confirmed == answer)
        };
        if answers.iter().all(holds) {
            Ok(())
        } else {
            Err(OwnerChanged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepared::PreparedInputKind;

    fn request(doc: &str) -> PreparedInputRequest {
        PreparedInputRequest::new(PreparedInputKind::AcpDecision, doc.as_bytes().to_vec())
    }

    fn answer(owner: &str) -> Arc<[u8]> {
        Arc::from(owner.as_bytes())
    }

    #[test]
    fn nothing_recorded_commits() {
        assert_eq!(ObservedOwners::default().confirm(&[]), Ok(()));
    }

    #[test]
    fn commit_only_if_every_answer_is_confirmed_as_itself() {
        let observed = ObservedOwners::default();
        observed.record(request("a"), answer("alice"));
        observed.record(request("b"), answer("null"));
        assert_eq!(observed.requests(), vec![request("a"), request("b")]);
        assert_eq!(
            observed.confirm(&[
                (request("a"), answer("alice")),
                (request("b"), answer("null"))
            ]),
            Ok(())
        );
        assert_eq!(
            observed.confirm(&[
                (request("a"), answer("bob")),
                (request("b"), answer("null"))
            ]),
            Err(OwnerChanged),
            "a changed answer conflicts"
        );
        assert_eq!(
            observed.confirm(&[(request("a"), answer("alice"))]),
            Err(OwnerChanged),
            "an unconfirmed answer conflicts"
        );
    }

    #[test]
    fn split_answers_to_one_request_never_commit() {
        let observed = ObservedOwners::default();
        observed.record(request("a"), answer("alice"));
        observed.record(request("a"), answer("bob"));
        assert_eq!(observed.requests(), vec![request("a")]);
        for owner in ["alice", "bob"] {
            assert_eq!(
                observed.confirm(&[(request("a"), answer(owner))]),
                Err(OwnerChanged)
            );
        }
    }
}
