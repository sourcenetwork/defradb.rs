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

    /// Checks a certified term that is not checked yet, and hands what it
    /// voided to the outbox. Runs on the task of a provider call that found
    /// the provider not admitted, and once at start, never on a task of its
    /// own: nothing but the provider holds its worker, so dropping the
    /// provider lets go of the worker's directory at once. A check that fails
    /// is run again by the next call. A journal closed because it could not
    /// vouch for what a term voided is rebuilt from the node's own state,
    /// read through `verifier`; one closed for any other reason stays closed.
    pub async fn check_with(&self, verifier: &dyn acp::RecoveryVerifier) {
        let certified = self.certified();
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
        let outcome = {
            let (journal, note) = worker.confirmed_and_note();
            let mut revise = |submission: alloy_primitives::B256, executed: bool| {
                note.revise(&format!("{submission:#x}"), executed)
            };
            follow::reconcile(journal, &self.chain, certified, &mut revise).await
        };
        self.follow.update(worker.confirmed());
        let owed = match outcome {
            Ok(Reconciled::Checked { owed }) => owed,
            Ok(Reconciled::Closed(reason)) => {
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
    /// next is read. Once the source says the pass for the certified term is
    /// over, that term is recorded as checked and the journal reopens: the
    /// outbox to send through, or `None` while the node stays closed. A pass
    /// cut short is taken up at its last recorded page by the next call.
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
        // The term the pass ends under: the source fences every page by the
        // term its verifier reports, so the last page names what was checked.
        let mut term = None;
        loop {
            let page = match source.next(verifier).await {
                Ok(Some(page)) => page,
                Ok(None) => break,
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
            term = Some(page.cursor.term);
            if let Err(error) = source.delivered(page.cursor).await {
                tracing::warn!(%error, "could not record Vera recovery progress");
                return None;
            }
        }
        let term = match term {
            Some(term) => term,
            // A pass an earlier call finished without recording it.
            None => match verifier.term().await {
                Ok(term) => term,
                Err(error) => {
                    tracing::warn!(%error, "Vera recovery term is not known");
                    return None;
                }
            },
        };
        if let Err(error) = worker.confirmed().recovered(term) {
            tracing::warn!(%error, "could not record a finished Vera recovery");
            return None;
        }
        self.follow.update(worker.confirmed());
        Some(sink)
    }
}

fn closed(reason: &str) {
    tracing::error!(target: "acp::audit", %reason, "Vera term check failed closed");
}
