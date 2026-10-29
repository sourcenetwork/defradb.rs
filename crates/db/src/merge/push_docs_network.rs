//! The network waits of a push, each run where network work runs: on the
//! network edge when the push runs on a pool worker, in place anywhere else
//! (`p2p::network::Network::hand_off`).

use std::time::Duration;

use p2p::message::{PushLogReply, PushLogRequest, PushSEArtifactsRequest};
use p2p::network::Network;
use p2p::transport::PeerId;
use p2p::P2PTransport;

/// How long a push waits for its peer to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// A send the network did not take: it is shutting down.
fn not_sent() -> p2p::Error {
    p2p::Error::Transport("the network is shutting down".to_string())
}

/// Sends one PushLog `request` to `peer_id` and waits for its reply.
pub(crate) async fn send_pushlog<T: P2PTransport>(
    network: &Network,
    transport: &T,
    peer_id: &PeerId,
    request: PushLogRequest,
) -> p2p::Result<PushLogReply> {
    let (transport, peer_id) = (transport.clone(), peer_id.clone());
    network
        .hand_off(move || async move { transport.send_two_stream_request(&peer_id, request).await })
        .await
        .unwrap_or_else(|| Err(not_sent()))
}

/// Sends SE artifacts to `peer_id`.
pub(crate) async fn send_se_artifacts<T: P2PTransport>(
    network: &Network,
    transport: &T,
    peer_id: &PeerId,
    request: PushSEArtifactsRequest,
) -> p2p::Result<()> {
    let (transport, peer_id) = (transport.clone(), peer_id.clone());
    network
        .hand_off(move || async move { transport.send_se_artifacts(&peer_id, request).await })
        .await
        .unwrap_or_else(|| Err(not_sent()))
}

/// Waits until `peer_id` is connected, polling every 50 ms for at most
/// [`CONNECT_TIMEOUT`].
pub(crate) async fn wait_for_peer<T: P2PTransport>(
    network: &Network,
    transport: &T,
    peer_id: &PeerId,
) -> Result<(), String> {
    let (transport, peer_id, clock) = (transport.clone(), peer_id.clone(), network.clone());
    let wait = move || async move {
        let start = web_time::Instant::now();
        let mut logged_conn_error = false;
        loop {
            let peers = match transport.connected_peers().await {
                Ok(peers) => peers,
                Err(e) => {
                    if !logged_conn_error {
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = %e,
                            "connected_peers check failed during replay wait"
                        );
                        logged_conn_error = true;
                    }
                    Vec::new()
                }
            };
            if peers.iter().any(|p| p == &peer_id) {
                return Ok(());
            }
            if start.elapsed() > CONNECT_TIMEOUT {
                return Err("timeout waiting for peer connection before push".to_string());
            }
            clock.sleep(Duration::from_millis(50)).await;
        }
    };
    network
        .hand_off(wait)
        .await
        .unwrap_or_else(|| Err(not_sent().to_string()))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::time::Duration;

    use p2p::message::PushLogReply;
    use p2p::network::Network;
    use p2p::transport::PeerId;

    use crate::merge::push_docs_replay::{ReplayPushConfig, ReplayPushGate};

    #[test]
    fn a_replay_paced_on_a_pool_worker_waits_on_the_edges_clock() {
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let network = Network::on(edge::testing::edge());
        let gate = ReplayPushGate::new(
            ReplayPushConfig {
                per_peer_rate_limit_burst: 1,
                per_peer_rate_limit_rate: 20.0,
                ..Default::default()
            },
            network,
        );
        let sent = pool
            .submit_and_wait(workers::Target::Worker(0), move || async move {
                let peer = PeerId::new("peer-1".to_string());
                let started = web_time::Instant::now();
                for _ in 0..2 {
                    gate.send_pushlog(&peer, async { Ok(PushLogReply::success("message")) })
                        .await
                        .unwrap();
                }
                started.elapsed()
            })
            .unwrap();
        assert!(
            sent >= Duration::from_millis(40),
            "the second send was paced: {sent:?}"
        );
        pool.shutdown_blocking().unwrap();
    }
}
