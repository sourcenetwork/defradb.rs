//! The provider's side of following its deployment's terms: what the light
//! client certified, checked against the worker's confirmed submissions
//! (`follow`), and what that leaves to owe again handed to the outbox.

use std::sync::{Arc, Weak};

use acp_light_client::AcpLightClient;
use regolith::sync::{Mutex, OnceCell};

use super::super::follow::{self, owed_again, Follow, ProviderChain, Reconciled};
use super::super::worker::NativeWorker;
use crate::provider::ProviderError;

pub(super) struct Follower {
    pub light_client: Arc<AcpLightClient>,
    pub chain: ProviderChain,
    pub worker: Arc<Mutex<NativeWorker>>,
    pub follow: Follow,
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

    /// Whether the provider may decide and send now ([`Follow::admit`]).
    pub fn admit(&self) -> Result<(), ProviderError> {
        self.follow.admit(self.certified())
    }

    /// The term last checked.
    pub fn epoch(&self) -> u64 {
        self.follow.epoch()
    }

    /// Checks a certified term that is not checked yet, and hands what it
    /// voided to the outbox. Runs from the light client observer and once at
    /// start; a check that fails is run again at the next one.
    pub async fn check(&self) {
        let certified = self.certified();
        if self.follow.is_closed()
            || (certified <= self.follow.epoch() && !self.follow.has_undelivered())
        {
            return;
        }
        let mut worker = self.worker.clone().lock_owned().await;
        let outcome = follow::reconcile(worker.confirmed(), &self.chain, certified).await;
        self.follow.update(worker.confirmed());
        let owed = match outcome {
            Ok(Reconciled::Checked { owed }) => owed,
            Ok(Reconciled::Closed(reason)) => return closed(&reason),
            Err(error) => {
                tracing::warn!(%error, term = certified, "Vera term not checked yet");
                return;
            }
        };
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

fn closed(reason: &str) {
    tracing::error!(target: "acp::audit", %reason, "Vera term check failed closed");
}
