//! The provider's side of following its deployment's terms: what the light
//! client certified, checked against the worker's confirmed submissions
//! (`follow`), and what that leaves to owe again handed to the outbox.

use std::sync::{Arc, Weak};

use acp_light_client::AcpLightClient;
use regolith::sync::{Mutex, OnceCell};

use events::{
    AcpTakeoverReconciledData, AcpTakeoverSubmission, AcpTakeoverUnrestorable, Bus, Message,
};

use super::super::follow::effect::Lost;
use super::super::follow::owed::{subject_string, token_actor};
use super::super::follow::report::{Applied, Unrestorable};
use super::super::follow::{self, owed_again, Follow, ProviderChain, Reconciled, TakeoverReport};
use super::super::worker::NativeWorker;
use crate::provider::ProviderError;

pub(super) struct Follower {
    pub light_client: Arc<AcpLightClient>,
    pub chain: ProviderChain,
    pub worker: Arc<Mutex<NativeWorker>>,
    pub follow: Follow,
    /// The node's bus, told once what the check of each term came to.
    pub bus: Option<Arc<dyn Bus>>,
    pub sink: OnceCell<Weak<dyn acp::OweAgain>>,
    /// The node's own access state, for a journal that cannot vouch for what
    /// a term voided.
    pub recovery: OnceCell<Weak<dyn acp::RecoverySource>>,
    /// Whether more than one node's outbox asked for what a term voids.
    pub shared: std::sync::atomic::AtomicBool,
    /// Whether the node holds an actor's key, so a change owed again as that
    /// actor is signed afresh at submission.
    pub holds_key: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl Follower {
    /// The newest term the light client holds a certificate of.
    pub fn certified(&self) -> u64 {
        self.light_client
            .header_chain()
            .term_certificate()
            .map_or(0, |certificate| certificate.change.term)
    }

    /// Whether the provider may decide and send now, and the term last
    /// checked ([`Follow::admit`]).
    pub fn admit(&self) -> Result<u64, ProviderError> {
        self.follow.admit(self.certified())
    }

    /// Whether what was admitted at `epoch` still is ([`Follow::still`]).
    pub fn still(&self, epoch: u64) -> Result<(), ProviderError> {
        self.follow.still(epoch, self.certified())
    }

    /// The term last checked.
    pub fn epoch(&self) -> u64 {
        self.follow.epoch()
    }

    /// The newest term the light client holds a certificate of, with the cut
    /// that certificate keeps.
    fn certificate(&self) -> Option<TermCut> {
        self.light_client
            .header_chain()
            .term_certificate()
            .map(|certificate| TermCut {
                term: certificate.change.term,
                height: certificate.change.cut.height,
                block: certificate.change.cut.block.to_string(),
            })
    }

    /// Checks a certified term that is not checked yet, and hands what it
    /// voided to the outbox. Runs on the task of a provider call that found
    /// the provider not admitted, and once at start, never on a task of its
    /// own: nothing but the provider holds its worker, so dropping the
    /// provider lets go of the worker's directory at once. A check that fails
    /// is run again by the next call. The call that checks a term tells the
    /// bus what it came to, once; a call that only delivers what the term
    /// left owed tells it nothing more, unless it fails closed. A journal
    /// closed because it could not vouch for what a term voided is rebuilt
    /// from the node's own state, read through `verifier`; one closed for any
    /// other reason stays closed.
    pub async fn check_with(&self, verifier: &dyn acp::RecoveryVerifier) {
        let certificate = self.certificate();
        let certified = certificate
            .as_ref()
            .map_or(0, |certificate| certificate.term);
        let settled = if self.follow.is_closed() {
            !self.follow.needs_recovery()
        } else {
            certified <= self.follow.epoch() && !self.follow.has_undelivered()
        };
        if settled {
            return;
        }
        let Ok(mut worker) =
            nuclei::time::timeout(self.chain.timeout, self.worker.clone().lock_owned()).await
        else {
            tracing::debug!(term = certified, "Vera term check waits for the worker");
            return;
        };
        let (outcome, reported) = {
            let (journal, note) = worker.confirmed_and_note();
            let reported = certificate
                .clone()
                .filter(|_| follow::first_check(journal, certified));
            let mut revise = |submission: alloy_primitives::B256, executed: bool| {
                note.revise(&format!("{submission:#x}"), executed)
            };
            (
                follow::reconcile(journal, &self.chain, certified, &mut revise).await,
                reported,
            )
        };
        self.follow.update(worker.confirmed());
        let report = match outcome {
            Ok(Reconciled::Checked { report }) => report,
            Ok(Reconciled::Closed { reason, report }) => {
                self.publish(reported.as_ref(), &report, Some(&reason));
                if worker.confirmed().recovery_needed().is_none() {
                    return closed(&reason);
                }
                let sink = self.recover(verifier, &mut worker, &reason).await;
                drop(worker);
                return self.resume(sink).await;
            }
            Err(error) => {
                tracing::warn!(%error, term = certified, "Vera term not checked yet");
                return;
            }
        };
        let effects = match self.owed_effects(&report.owed) {
            Ok(effects) => effects,
            Err(reason) => {
                if let Err(error) = worker.confirmed().close(reason.clone()) {
                    // The term is checked and its owed changes wait, so the
                    // node still admits nothing. The event says closed only
                    // once the journal does: the next call closes again and
                    // tells the bus then.
                    tracing::warn!(%error, "could not record a failed term check");
                    self.publish(reported.as_ref(), &report, None);
                    return;
                }
                self.follow.update(worker.confirmed());
                // A call that only delivers says so too: the term's report
                // named these changes as owed, and now nothing will send them.
                self.publish(
                    reported.as_ref().or(certificate.as_ref()),
                    &report,
                    Some(&reason),
                );
                return closed(&reason);
            }
        };
        self.publish(reported.as_ref(), &report, None);
        let sink = if effects.is_empty() {
            None
        } else {
            let Some(sink) = self.sink.get().and_then(Weak::upgrade) else {
                tracing::warn!(
                    owed = effects.len(),
                    "changes a Vera term voided wait for the outbox to owe them again"
                );
                return;
            };
            if let Err(error) = sink.owe_again(&effects).await {
                tracing::warn!(%error, "changes a Vera term voided are not owed again yet");
                return;
            }
            if let Err(error) = worker.confirmed().delivered() {
                tracing::warn!(%error, "could not record changes owed again");
                return;
            }
            self.follow.update(worker.confirmed());
            Some(sink)
        };
        drop(worker);
        self.resume(sink).await;
    }

    /// The provider decides again: nothing read before the term is served,
    /// and what was handed to `sink` is sent. The worker is not held here:
    /// the submissions take it.
    async fn resume(&self, sink: Option<Arc<dyn acp::OweAgain>>) {
        if let Ok(state) = self.light_client.header_chain().fresh_state() {
            self.light_client
                .cache()
                .invalidate_stale(state.module_state_root);
        }
        if let Some(sink) = sink {
            sink.settle_owed().await;
        }
    }

    /// Rebuild the deployment from the node's own state, one bounded page at
    /// a time, handing each page to the outbox and recording it before the
    /// next is read. Once the source says the pass for a term is over, that
    /// term is recorded as checked and the journal reopens: the outbox to
    /// send through, or `None` while the node stays closed. A term certified
    /// since is still unchecked, so admission waits for it. A pass cut short
    /// is taken up at its last recorded page by the next call.
    async fn recover(
        &self,
        verifier: &dyn acp::RecoveryVerifier,
        worker: &mut regolith::sync::OwnedMutexGuard<NativeWorker>,
        reason: &str,
    ) -> Option<Arc<dyn acp::OweAgain>> {
        if self.shared.load(std::sync::atomic::Ordering::Acquire) {
            closed(reason);
            return None;
        }
        let Some(source) = self.recovery.get().and_then(Weak::upgrade) else {
            tracing::warn!(%reason, "no Vera recovery source is installed");
            return None;
        };
        let Some(sink) = self.sink.get().and_then(Weak::upgrade) else {
            tracing::warn!(%reason, "Vera recovery waits for the node's outbox");
            return None;
        };
        let term = loop {
            let page = match source.next(verifier).await {
                Ok(acp::RecoveryStep::Page(page)) => page,
                Ok(acp::RecoveryStep::Done { term }) => break term,
                Err(error) => {
                    tracing::warn!(%error, %reason, "Vera recovery pass did not finish");
                    return None;
                }
            };
            if !page.effects.is_empty() {
                if let Err(error) = sink.owe_again(&page.effects).await {
                    tracing::warn!(%error, "Vera recovery state was not owed again");
                    return None;
                }
            }
            for gap in &page.unrestored {
                tracing::warn!(target: "acp::audit", gap, "Vera recovery could not restore state");
            }
            if let Err(error) = source.delivered(page.cursor).await {
                tracing::warn!(%error, "could not record Vera recovery progress");
                return None;
            }
        };
        // A second node's outbox that asked meanwhile took no part in the pass.
        if self.shared.load(std::sync::atomic::Ordering::Acquire) {
            closed(reason);
            return None;
        }
        if let Err(error) = worker.confirmed().recovered(term) {
            tracing::warn!(%error, "could not record a finished Vera recovery");
            return None;
        }
        self.follow.update(worker.confirmed());
        Some(sink)
    }

    /// `owed` as the outbox records it, or why the node cannot owe it and
    /// fails closed: several nodes share the provider, or a change's
    /// delegation names no actor.
    fn owed_effects(&self, owed: &[Lost]) -> Result<Vec<acp::VoidedEffect>, String> {
        if owed.is_empty() {
            return Ok(Vec::new());
        }
        if self.shared.load(std::sync::atomic::Ordering::Acquire) {
            return Err(
                "the Vera deployment switched its active device and voided changes \
                 this provider confirmed, and the provider serves several nodes: it \
                 cannot tell whose outbox owes them. Make the changes again, then \
                 restart each node with a provider of its own and a new worker \
                 directory"
                    .to_owned(),
            );
        }
        owed.iter()
            .map(|lost| {
                owed_again(lost, |did| (self.holds_key)(did)).ok_or_else(|| {
                    let (policy, resource, object) = lost.object();
                    format!(
                        "the Vera deployment switched its active device and voided a change on \
                         {policy}/{resource}/{object} this node confirmed, whose delegation names \
                         no actor it can owe it as; access may differ from what the node \
                         committed. Make the change again, then restart the node with a new \
                         worker directory"
                    )
                })
            })
            .collect()
    }

    /// Tells the node what the check of term `reported` came to, and why it
    /// failed closed when it did. `None` when this call has nothing to add
    /// to what the call that checked the term told it.
    fn publish(&self, reported: Option<&TermCut>, report: &TakeoverReport, closed: Option<&str>) {
        let (Some(bus), Some(cut)) = (&self.bus, reported) else {
            return;
        };
        bus.publish(Message::acp_takeover_reconciled(takeover_report(
            cut, report, closed,
        )));
    }
}

/// A certified term and the cut its certificate keeps, read together so a
/// report never pairs one term with another's cut.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TermCut {
    term: u64,
    height: u64,
    block: String,
}

fn submission(lost: &Lost) -> AcpTakeoverSubmission {
    let (policy_id, resource, object) = lost.object();
    let (kind, relation, subject, add) = match lost {
        Lost::Registration { .. } => ("registration", None, None, None),
        Lost::Archive { .. } => ("archive", None, None, None),
        Lost::Relationship {
            relation,
            subject,
            add,
            ..
        } => (
            "relationship",
            Some(relation.clone()),
            subject_string(subject),
            Some(*add),
        ),
    };
    AcpTakeoverSubmission {
        kind: kind.into(),
        policy_id: policy_id.to_owned(),
        resource: resource.to_owned(),
        object: object.to_owned(),
        relation,
        subject,
        add,
        actor: token_actor(lost.token()),
    }
}

fn applied(applied: &Applied) -> AcpTakeoverSubmission {
    match applied {
        Applied::Change(lost) => submission(lost),
        Applied::PolicyCreation { policy_id, creator } => AcpTakeoverSubmission {
            kind: "policy-creation".into(),
            policy_id: policy_id.clone(),
            resource: String::new(),
            object: String::new(),
            relation: None,
            subject: None,
            add: None,
            actor: Some(creator.clone()),
        },
    }
}

fn unrestorable(item: &Unrestorable) -> AcpTakeoverUnrestorable {
    let Unrestorable::PolicyCreation {
        policy_id,
        creator,
        reason,
    } = item;
    AcpTakeoverUnrestorable {
        kind: "policy-creation".into(),
        policy_id: policy_id.clone(),
        resource: String::new(),
        object: String::new(),
        actor: Some(creator.clone()),
        reason: reason.clone(),
    }
}

/// `report` as the event carries it: every entry of each list, in order.
fn takeover_report(
    cut: &TermCut,
    report: &TakeoverReport,
    closed: Option<&str>,
) -> AcpTakeoverReconciledData {
    AcpTakeoverReconciledData {
        term: cut.term,
        cut_height: cut.height,
        cut_block: cut.block.clone(),
        closed: closed.map(str::to_owned),
        applied_again: report.applied.iter().map(applied).collect(),
        owed_again: report.owed.iter().map(submission).collect(),
        unrestorable: report.unrestorable.iter().map(unrestorable).collect(),
    }
}

fn closed(reason: &str) {
    tracing::error!(target: "acp::audit", %reason, "Vera term check failed closed");
}

#[cfg(test)]
#[path = "terms_tests.rs"]
mod tests;
