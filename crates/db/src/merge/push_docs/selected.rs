//! Flow control for one explicit selection, independent of durable replication retries.

use std::time::Duration;

use futures::future::{select, Either};
use p2p::message::{PushLogReply, PushLogRequest};
use p2p::network::Network;
use p2p::transport::PeerId;
use p2p::P2PTransport;
use regolith::sync::OnceCell;
use web_time::Instant;

use super::stop::{reply_requests_pause, SelectedPushStop};
use crate::merge::push_docs_replay::{ReplayPushGate, ReplayPushSendError};

/// One current head's deadline starts at its first actual send, after initial
/// pacing and slot admission. Every retry and receiver pause spends the same
/// allowance. A fresh acknowledged head gives the next head its own deadline.
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

    fn remaining(&self) -> Result<Option<Duration>, ReplayPushSendError> {
        let Some(started) = self.started.get() else {
            return Ok(None);
        };
        self.within
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
            .map(Some)
            .ok_or(ReplayPushSendError::HeadTimeout {
                timeout: self.within,
            })
    }

    async fn wait(
        &self,
        delay: Duration,
        stop: &SelectedPushStop,
    ) -> Result<(), ReplayPushSendError> {
        if stop.is_stopped() {
            return Err(ReplayPushSendError::Stopped);
        }
        let delay = self.remaining()?.map_or(delay, |left| delay.min(left));
        let cancelled = stop.cancelled();
        let waiting = self.network.sleep(delay);
        futures::pin_mut!(cancelled, waiting);
        if matches!(select(cancelled, waiting).await, Either::Left(_)) {
            return Err(ReplayPushSendError::Stopped);
        }
        self.remaining()?;
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
        let reply = match deadline.remaining()? {
            None => sending.await,
            Some(left) => deadline.network.timeout(left, sending).await.map_err(|_| {
                ReplayPushSendError::HeadTimeout {
                    timeout: deadline.within,
                }
            })?,
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
