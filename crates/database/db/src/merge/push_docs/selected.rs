//! Flow control for one explicit selection, independent of durable replication retries.

use std::time::Duration;

use futures::future::{select, Either};
use p2p::message::{PushLogReply, PushLogRequest};
use p2p::network::Network;
use p2p::rt::time::Instant;
use p2p::transport::PeerId;
use p2p::P2PTransport;
use regolith::sync::OnceCell;

use super::stop::{reply_requests_pause, SelectedPushStop};
use crate::merge::push_docs_replay::{ReplayPushGate, ReplayPushSendError};

/// One current head's deadline starts at its first actual send, after initial
/// pacing and slot admission, on the network clock its waits run on. Every
/// retry and receiver pause spends the same allowance. A head the allowance
/// cannot see through is deferred: it fails alone and the rest of the
/// selection is still tried, because the receiver is answering
/// (`SelectedPushIsolation.tla` PauseExpired). A fresh acknowledged head gives
/// the next head its own deadline.
pub(super) struct HeadDeadline {
    started: OnceCell<Instant>,
    within: Duration,
    network: Network,
}

impl HeadDeadline {
    pub(super) fn new(within: Duration, network: Network) -> Self {
        Self {
            started: OnceCell::new(),
            within: within.max(Duration::from_millis(1)),
            network,
        }
    }

    fn start(&self) {
        self.started.get_or_init_racy(Instant::now);
    }

    /// What is left of the allowance, or `None` before the first send.
    fn remaining(&self) -> Option<Duration> {
        let started = self.started.get()?;
        Some(self.within.saturating_sub(started.elapsed()))
    }

    fn deferred(&self) -> ReplayPushSendError {
        ReplayPushSendError::HeadDeferred {
            timeout: self.within,
        }
    }

    /// Wait out a receiver pause of `delay`. A pause the head's remaining
    /// allowance cannot cover defers the head at once, since no resend could
    /// fit in what is left.
    async fn wait(
        &self,
        delay: Duration,
        stop: &SelectedPushStop,
    ) -> Result<(), ReplayPushSendError> {
        if stop.is_stopped() {
            return Err(ReplayPushSendError::Stopped);
        }
        if self.remaining().is_some_and(|left| delay >= left) {
            return Err(self.deferred());
        }
        let cancelled = stop.cancelled();
        let waiting = self.network.sleep(delay);
        futures::pin_mut!(cancelled, waiting);
        if matches!(select(cancelled, waiting).await, Either::Left(_)) {
            return Err(ReplayPushSendError::Stopped);
        }
        Ok(())
    }
}

/// Retry only an explicit receiver pause, sending this same still-unacknowledged
/// head. The gate's peer-wide pause paces every task. Only this head's first
/// send starts its fixed deadline, without inventing a continuous retry marker.
pub(super) async fn send<T>(
    transport: &T,
    gate: &ReplayPushGate,
    deadline: &HeadDeadline,
    stop: &SelectedPushStop,
    peer: &PeerId,
    request: &PushLogRequest,
) -> Result<PushLogReply, ReplayPushSendError>
where
    T: P2PTransport,
{
    loop {
        if stop.is_stopped() {
            return Err(ReplayPushSendError::Stopped);
        }
        let sending = gate.send_pushlog_with_admission(
            peer,
            async {
                if stop.is_stopped() {
                    Err(ReplayPushSendError::Stopped)
                } else {
                    deadline.start();
                    Ok(())
                }
            },
            transport.send_two_stream_request(peer, request.clone()),
        );
        // A resend only happens after a receiver pause, so a head that runs out
        // of allowance here is deferred, not taken for a silent peer: a first
        // send carries the gate's own send deadline instead.
        let reply = match deadline.remaining() {
            None => sending.await,
            Some(left) if left.is_zero() => return Err(deadline.deferred()),
            Some(left) => deadline
                .network
                .timeout(left, sending)
                .await
                .map_err(|_| deadline.deferred())?,
        };
        let delay = match reply {
            Ok(reply) if reply_requests_pause(&reply) => {
                let delay = reply
                    .retry_after()
                    .unwrap_or(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL);
                gate.pause_peer_for(peer, delay);
                delay
            }
            Err(ReplayPushSendError::Backpressure { retry_after }) => retry_after,
            result => return result,
        };
        deadline.wait(delay, stop).await?;
    }
}
