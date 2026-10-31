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

/// The first pause starts a fixed allowance, shared across this selection's
/// tasks. More hints or acknowledgements never extend it. Ordinary sends keep
/// their existing per-send deadline when a receiver has not asked to pause.
pub(super) struct PauseBudget {
    started: OnceCell<Instant>,
    within: Duration,
    network: Network,
}

impl PauseBudget {
    pub(super) fn new(within: Duration, network: Network) -> Self {
        Self {
            started: OnceCell::new(),
            within: within.max(Duration::from_millis(1)),
            network,
        }
    }

    async fn wait(
        &self,
        delay: Duration,
        stop: &SelectedPushStop,
    ) -> Result<(), ReplayPushSendError> {
        if stop.is_stopped() {
            return Err(ReplayPushSendError::Stopped);
        }
        let started = self.started.get_or_init_racy(Instant::now);
        let Some(remaining) = self
            .within
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
        else {
            return Err(ReplayPushSendError::PauseTimeout {
                timeout: self.within,
            });
        };
        let cancelled = stop.cancelled();
        let waiting = self.network.sleep(delay.min(remaining));
        futures::pin_mut!(cancelled, waiting);
        if matches!(select(cancelled, waiting).await, Either::Left(_)) {
            return Err(ReplayPushSendError::Stopped);
        }
        if started.elapsed() >= self.within {
            return Err(ReplayPushSendError::PauseTimeout {
                timeout: self.within,
            });
        }
        Ok(())
    }
}

/// Retry only an explicit receiver pause, sending this same still-unacknowledged
/// head. The gate's peer-wide pause paces every task, and the fixed budget bounds
/// a receiver that keeps nacking without inventing a continuous retry marker.
pub(super) async fn send<T>(
    transport: &T,
    gate: &ReplayPushGate,
    budget: &PauseBudget,
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
        let reply = gate
            .send_pushlog_with_admission(
                peer,
                async {
                    if stop.is_stopped() {
                        Err(ReplayPushSendError::Stopped)
                    } else {
                        Ok(())
                    }
                },
                transport.send_two_stream_request(peer, request.clone()),
            )
            .await;
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
        budget.wait(delay, stop).await?;
    }
}
