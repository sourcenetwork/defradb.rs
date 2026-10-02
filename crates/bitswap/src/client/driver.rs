//! Runs a [`Client`] against the network: spawns the sends, applies their results, fires request timeouts
//! and applies keep-alive changes in order.
//!
//! Every wake source (send results, the next deadline) is polled with the swarm's context, so a result or an
//! expiry always wakes the swarm.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use cid::Cid;
use libp2p::PeerId;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, timeout, Instant, Sleep};
use tracing::debug;

use super::{Client, KeepAlive};
use crate::network::Network;
use crate::protocol::ProtocolId;

const KEEP_ALIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug)]
struct SendDone {
    peer: PeerId,
    requests: Vec<(Cid, u64)>,
    ok: bool,
}

#[derive(Debug)]
pub(crate) struct Driver {
    network: Network,
    results: mpsc::UnboundedReceiver<SendDone>,
    results_tx: mpsc::UnboundedSender<SendDone>,
    keep_alive: mpsc::UnboundedSender<KeepAlive>,
    sleep: Pin<Box<Sleep>>,
    armed: Option<Instant>,
    waker: Option<Waker>,
}

impl Driver {
    /// Spawns the keep-alive task on the current tokio runtime.
    pub(crate) fn new(network: Network) -> Self {
        let (results_tx, results) = mpsc::unbounded_channel();
        let (keep_alive, ops) = mpsc::unbounded_channel();
        tokio::spawn(run_keep_alive(network.clone(), ops));
        Driver {
            network,
            results,
            results_tx,
            keep_alive,
            sleep: Box::pin(sleep_until(Instant::now())),
            armed: None,
            waker: None,
        }
    }

    /// Wakes the swarm after the client was changed from outside a poll.
    pub(crate) fn wake(&self) {
        if let Some(waker) = &self.waker {
            waker.wake_by_ref();
        }
    }

    pub(crate) fn poll(
        &mut self,
        client: &mut Client,
        protocol: impl Fn(&PeerId) -> Option<ProtocolId>,
        cx: &mut Context<'_>,
    ) {
        if self.waker.as_ref().is_none_or(|w| !w.will_wake(cx.waker())) {
            self.waker = Some(cx.waker().clone());
        }
        while let Poll::Ready(Some(done)) = self.results.poll_recv(cx) {
            client.send_done(done.peer, &done.requests, done.ok);
        }
        self.poll_deadline(client, cx);
        for outgoing in client.take_ready(protocol) {
            let network = self.network.clone();
            let results = self.results_tx.clone();
            tokio::spawn(async move {
                let peer = outgoing.peer;
                let ok = match network.send_message(peer, outgoing.message).await {
                    Ok(()) => true,
                    Err(error) => {
                        debug!(%peer, %error, "client send failed");
                        false
                    }
                };
                let done = SendDone {
                    peer,
                    requests: outgoing.requests,
                    ok,
                };
                if results.send(done).is_err() {
                    debug!(%peer, "client send result dropped, behaviour is gone");
                }
            });
        }
        for change in client.take_keep_alive() {
            if self.keep_alive.send(change).is_err() {
                debug!("keep-alive task is gone");
            }
        }
    }

    fn poll_deadline(&mut self, client: &mut Client, cx: &mut Context<'_>) {
        while let Some(deadline) = client.next_deadline() {
            if self.armed != Some(deadline) {
                self.sleep.as_mut().reset(deadline);
                self.armed = Some(deadline);
            }
            if self.sleep.as_mut().poll(cx).is_pending() {
                return;
            }
            self.armed = None;
            client.expire();
        }
        self.armed = None;
    }
}

async fn run_keep_alive(network: Network, mut ops: mpsc::UnboundedReceiver<KeepAlive>) {
    while let Some(op) = ops.recv().await {
        let applied = timeout(KEEP_ALIVE_TIMEOUT, async {
            match op {
                KeepAlive::Protect(peer) => network.protect_peer(peer).await,
                KeepAlive::Unprotect(peer) => {
                    network.unprotect_peer(peer).await;
                }
            }
        })
        .await;
        if applied.is_err() {
            debug!(?op, "keep-alive change timed out");
        }
    }
}
