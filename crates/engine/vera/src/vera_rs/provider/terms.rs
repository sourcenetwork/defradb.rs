//! The provider's side of following its deployment's terms: what the light
//! client certified, checked against the worker's confirmed submissions
//! (`follow`), and what that leaves to owe again handed to the outbox.

use std::sync::{Arc, Weak};

use acp_light_client::AcpLightClient;
use regolith::sync::{Mutex, OnceCell};

use events::{
    AcpTakeoverReconciledData, AcpTakeoverSubmission, AcpTakeoverUnrestorable, Bus, Message,
};

use super::super::follow::{self, owed_again, Follow, ProviderChain, Reconciled};
use super::super::worker::NativeWorker;
use crate::provider::ProviderError;

pub(super) struct Follower {
    pub light_client: Arc<AcpLightClient>,
    pub chain: ProviderChain,
    pub worker: Arc<Mutex<NativeWorker>>,
    pub follow: Follow,
    /// The node's bus, told once what one takeover's replay came to.
    pub bus: Option<Arc<dyn Bus>>,
    pub sink: OnceCell<Weak<dyn acp::OweAgain>>,
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

    /// The cut the newest certified term keeps, as `(height, block)`.
    pub fn cut(&self) -> (u64, String) {
        self.light_client.header_chain().term_certificate().map_or(
            (0, alloy_primitives::B256::ZERO.to_string()),
            |certificate| {
                (
                    certificate.change.cut.height,
                    certificate.change.cut.block.to_string(),
                )
            },
        )
    }

    /// Checks a certified term that is not checked yet, and hands what it
    /// voided to the outbox. Runs on the task of a provider call that found
    /// the provider not admitted, and once at start, never on a task of its
    /// own: nothing but the provider holds its worker, so dropping the
    /// provider lets go of the worker's directory at once. A check that fails
    /// is run again by the next call.
    pub async fn check(&self) {
        let certified = self.certified();
        if self.follow.is_closed()
            || (certified <= self.follow.epoch() && !self.follow.has_undelivered())
        {
            return;
        }
        let Ok(mut worker) =
            nuclei::time::timeout(self.chain.timeout, self.worker.clone().lock_owned()).await
        else {
            tracing::debug!(term = certified, "Vera term check waits for the worker");
            return;
        };
        let outcome = {
            let (journal, note) = worker.confirmed_and_note();
            let mut revise = |submission: alloy_primitives::B256, executed: bool| {
                note.revise(&format!("{submission:#x}"), executed)
            };
            follow::reconcile(journal, &self.chain, certified, &mut revise).await
        };
        self.follow.update(worker.confirmed());
        let (owed, report) = match outcome {
            Ok(Reconciled::Checked { mut report }) => {
                let owed = std::mem::take(&mut report.owed);
                (owed, report)
            }
            Ok(Reconciled::Closed { reason, report }) => {
                self.publish(certified, &report);
                return closed(&reason);
            }
            Err(error) => {
                tracing::warn!(%error, term = certified, "Vera term not checked yet");
                return;
            }
        };
        self.publish(certified, &report);
        let sink = if owed.is_empty() {
            None
        } else if self.shared.load(std::sync::atomic::Ordering::Acquire) {
            let reason = "the Vera deployment switched its active device and voided changes \
                          this provider confirmed, and the provider serves several nodes: it \
                          cannot tell whose outbox owes them. Make the changes again, then \
                          restart each node with a provider of its own and a new worker \
                          directory"
                .to_owned();
            if let Err(error) = worker.confirmed().close(reason.clone()) {
                tracing::warn!(%error, "could not record a failed term check");
            }
            self.follow.update(worker.confirmed());
            return closed(&reason);
        } else {
            let Some(sink) = self.sink.get().and_then(Weak::upgrade) else {
                tracing::warn!(
                    owed = owed.len(),
                    "changes a Vera term voided wait for the outbox to owe them again"
                );
                return;
            };
            let mut effects = Vec::with_capacity(owed.len());
            for lost in &owed {
                let Some(effect) = owed_again(lost, |did| (self.holds_key)(did)) else {
                    let (policy, resource, object) = lost.object();
                    let reason = format!(
                        "the Vera deployment switched its active device and voided a change on \
                         {policy}/{resource}/{object} this node confirmed, whose delegation names \
                         no actor it can owe it as; access may differ from what the node \
                         committed. Make the change again, then restart the node with a new \
                         worker directory"
                    );
                    if let Err(error) = worker.confirmed().close(reason.clone()) {
                        tracing::warn!(%error, "could not record a failed term check");
                    }
                    self.follow.update(worker.confirmed());
                    return closed(&reason);
                };
                effects.push(effect);
            }
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
        if let Ok(state) = self.light_client.header_chain().fresh_state() {
            self.light_client
                .cache()
                .invalidate_stale(state.module_state_root);
        }
        if let Some(sink) = sink {
            sink.settle_owed().await;
        }
    }
}

impl Follower {
    /// Tell the node once what one takeover's replay came to. The cut comes
    /// from the certificate the light client verified; the outcomes come
    /// from the replay itself.
    fn publish(&self, term: u64, report: &super::super::follow::TakeoverReport) {
        let Some(bus) = &self.bus else {
            return;
        };
        let (cut_height, cut_block) = self.cut();
        bus.publish(Message::acp_takeover_reconciled(takeover_report(
            term, cut_height, cut_block, report,
        )));
    }
}

fn takeover_report(
    term: u64,
    cut_height: u64,
    cut_block: String,
    report: &super::super::follow::TakeoverReport,
) -> AcpTakeoverReconciledData {
    let to_submission = |lost: &super::super::follow::effect::Lost| {
        let (policy_id, resource, object) = lost.object();
        AcpTakeoverSubmission {
            kind: match lost {
                super::super::follow::effect::Lost::Registration { .. } => "registration".into(),
                super::super::follow::effect::Lost::Relationship { .. } => "relationship".into(),
            },
            policy_id: policy_id.to_owned(),
            resource: resource.to_owned(),
            object: object.to_owned(),
            relation: match lost {
                super::super::follow::effect::Lost::Relationship { relation, .. } => {
                    Some(relation.clone())
                }
                _ => None,
            },
            subject: match lost {
                super::super::follow::effect::Lost::Relationship { subject, .. } => {
                    super::super::follow::owed::subject_string(subject)
                }
                _ => None,
            },
            add: match lost {
                super::super::follow::effect::Lost::Relationship { add, .. } => Some(*add),
                _ => None,
            },
            actor: super::super::follow::owed::token_actor(lost.token()),
        }
    };
    let applied_again = report
        .applied
        .iter()
        .map(|applied| match applied {
            super::super::follow::report::Applied::Change(lost) => to_submission(lost),
            super::super::follow::report::Applied::PolicyCreation { policy_id } => {
                AcpTakeoverSubmission {
                    kind: "policy-creation".into(),
                    policy_id: policy_id.clone(),
                    resource: String::new(),
                    object: String::new(),
                    relation: None,
                    subject: None,
                    add: None,
                    actor: None,
                }
            }
            super::super::follow::report::Applied::Archive {
                policy_id,
                resource,
                object,
            } => AcpTakeoverSubmission {
                kind: "archive".into(),
                policy_id: policy_id.clone(),
                resource: resource.clone(),
                object: object.clone(),
                relation: None,
                subject: None,
                add: None,
                actor: None,
            },
        })
        .collect();
    let owed_again = report.owed.iter().map(to_submission).collect();
    let unrestorable = report
        .unrestorable
        .iter()
        .map(|item| match item {
            super::super::follow::report::Unrestorable::PolicyCreation { policy_id, reason } => {
                AcpTakeoverUnrestorable {
                    kind: "policy-creation".into(),
                    policy_id: policy_id.clone(),
                    resource: String::new(),
                    object: String::new(),
                    reason: reason.clone(),
                }
            }
            super::super::follow::report::Unrestorable::Archive {
                policy_id,
                resource,
                object,
                reason,
            } => AcpTakeoverUnrestorable {
                kind: "archive".into(),
                policy_id: policy_id.clone(),
                resource: resource.clone(),
                object: object.clone(),
                reason: reason.clone(),
            },
        })
        .collect();
    AcpTakeoverReconciledData {
        term,
        cut_height,
        cut_block,
        applied_again,
        owed_again,
        unrestorable,
    }
}

fn closed(reason: &str) {
    tracing::error!(target: "acp::audit", %reason, "Vera term check failed closed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vera_rs::follow::report::{TakeoverReport, Unrestorable};

    fn policy_id() -> String {
        "0xlost".into()
    }

    #[test]
    fn a_lost_policy_creation_is_named_in_the_event() {
        let report = TakeoverReport {
            unrestorable: vec![Unrestorable::PolicyCreation {
                policy_id: policy_id(),
                reason: "voided".into(),
            }],
            ..TakeoverReport::default()
        };
        let data = takeover_report(2, 5, "0xcut".into(), &report);
        assert_eq!(data.unrestorable.len(), 1);
        assert_eq!(data.unrestorable[0].kind, "policy-creation");
        assert_eq!(data.unrestorable[0].policy_id, policy_id());
        assert_eq!(data.unrestorable[0].reason, "voided");
    }
}
