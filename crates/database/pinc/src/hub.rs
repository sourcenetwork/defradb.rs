//! Hubs: what pinc topic broadcast runs through.
//!
//! A node names its hubs (pinc's relays) when its endpoint is built;
//! subscriptions and publishes go to them, and they push each message to the
//! topic's members. [`spawn_hub`] runs one with no node on top, for a test
//! process or an operator.
//!
//! A hub carries topics only, unless [`HubConfig::relay`] gives it pinc's
//! relay role. Then it also keeps each member's signed record, answers a
//! member's lookup of a peer by id, tells members the address it sees them
//! at, and forwards a session's packets between two members that cannot
//! reach each other directly (behind NATs) until hole punching finds a
//! direct path. Members reach it over UDP, through the hub's own endpoint;
//! it serves no TCP or WebSocket carriers, so a member whose UDP is blocked
//! is not reached.

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use pinc::Hosted;
use tracing::{debug, info, warn};

use p2p::error::{Error, Result};
use p2p::rt::oneshot;

use super::addr::{advertised, parse_node_addr};
use super::endpoint::NucleiIo;
use super::services::ALPN_MUX;
use super::wire::pinc_error;

/// pinc's bound on the relays an endpoint names.
const MAX_HUBS: usize = 4;

/// The largest payload one topic message carries: pinc's default, and
/// gossipsub's too. A publish over it fails with `GossipTooLarge`, and the
/// KMS splits a key batch whose reply would be larger
/// ([`PincTransport::max_raw_publish`](super::PincTransport)).
pub(super) const MAX_TOPIC_MESSAGE: usize = 64 * 1024;

/// The endpoint's sizes: `connections` sessions, room for a topic per
/// document (pinc's ceiling), and [`MAX_TOPIC_MESSAGE`].
pub(super) fn limits(connections: usize) -> pinc::Limits {
    pinc::Limits {
        connections,
        topics: 4096,
        topic_message: MAX_TOPIC_MESSAGE,
        ..pinc::Limits::default()
    }
}

/// The hubs `tickets` name, each `<peer id>@<udp address>`.
pub(super) fn hubs(tickets: &[String]) -> Result<Vec<pinc::Relay>> {
    if tickets.len() > MAX_HUBS {
        return Err(Error::InvalidConfig(format!(
            "a pinc node names at most {MAX_HUBS} hubs, not {}",
            tickets.len()
        )));
    }
    tickets
        .iter()
        .map(|ticket| {
            let addr = parse_node_addr(ticket)?;
            let udp = addr.direct.first().copied().ok_or_else(|| {
                Error::InvalidConfig(format!("pinc hub {ticket} has no UDP address"))
            })?;
            Ok(pinc::Relay::new(addr.node).with_udp(udp))
        })
        .collect()
}

/// How a hub runs.
#[derive(Clone)]
pub struct HubConfig {
    /// The Ed25519 seed the hub's peer id comes from.
    pub seed: zeroize::Zeroizing<[u8; 32]>,
    /// The UDP address to bind. A wildcard IP lists every interface's
    /// address in the ticket, as a node does.
    pub bind: SocketAddr,
    /// Addresses members reach the hub at that are on none of its
    /// interfaces, listed first in its ticket.
    pub public_addrs: Vec<SocketAddr>,
    /// Run pinc's relay role beside the hub: members behind NATs reach each
    /// other through it. Off by default: the hub then carries topics only.
    pub relay: bool,
}

impl HubConfig {
    pub fn new(seed: zeroize::Zeroizing<[u8; 32]>, bind: SocketAddr) -> Self {
        Self {
            seed,
            bind,
            public_addrs: Vec::new(),
            relay: false,
        }
    }
}

impl std::fmt::Debug for HubConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubConfig")
            .field("bind", &self.bind)
            .field("public_addrs", &self.public_addrs)
            .field("relay", &self.relay)
            .finish_non_exhaustive()
    }
}

/// Run a hub on a thread of `edge` as `config` says. Returns its ticket for
/// [`PincEndpointConfig::hubs`](super::PincEndpointConfig) and the task,
/// which runs until the edge closes. A hub bound to a wildcard on a host
/// with no address but loopback, and named no public address, fails: its
/// ticket would name no address another host can reach.
pub async fn spawn_hub(config: HubConfig, edge: edge::Edge) -> Result<(String, edge::Task<()>)> {
    let (ready_tx, ready_rx) = oneshot::channel::<Result<String>>();
    let closing = edge.clone();
    let task = edge
        .spawn_pinned(move || async move {
            let bind = config.bind;
            let socket = match nuclei::Handle::<std::net::UdpSocket>::bind(bind).await {
                Ok(socket) => socket,
                Err(error) => {
                    let _ = ready_tx.send(Err(Error::Transport(format!(
                        "failed to bind pinc hub socket {bind}: {error}"
                    ))));
                    return;
                }
            };
            let local = socket.local_addr().unwrap_or(bind);
            let direct = advertised(local, &config.public_addrs);
            if direct.is_empty() {
                let _ = ready_tx.send(Err(Error::InvalidConfig(format!(
                    "pinc hub bound to {local} has no address another host can reach: \
                     name one with --public-addr, or bind a specific address"
                ))));
                return;
            }
            let io = NucleiIo {
                socket: Rc::new(socket),
            };
            let base = match pinc::Pinc::builder().seed(&config.seed) {
                Ok(builder) => builder
                    .alpn(ALPN_MUX)
                    .local_addr(local)
                    .limits(limits(pinc::Limits::default().connections))
                    .hub(),
                Err(error) => {
                    let _ = ready_tx.send(Err(pinc_error("pinc hub", error)));
                    return;
                }
            };
            if config.relay {
                let built = base
                    .relay_role(pinc::relay::Relay::builder(), udp_only_carriers())
                    .build_io(io);
                serve_hub(built, direct, ready_tx, closing).await;
            } else {
                serve_hub(base.build_io(io), direct, ready_tx, closing).await;
            }
        })
        .within(Duration::MAX);
    let ticket = ready_rx
        .await
        .map_err(|_| Error::Transport("the pinc hub stopped before it was bound".to_owned()))??;
    Ok((ticket, task))
}

/// The relay role's carriers: none. Members reach the relay over UDP
/// through the hub's endpoint, so no stream is ever accepted, and the
/// relay lists no peer relays to open one to.
#[allow(clippy::type_complexity)]
fn udp_only_carriers() -> pinc::relay::CarrierFns<
    impl AsyncFn() -> pinc::Result<((), pinc::CarrierKind)>,
    impl AsyncFn(&pinc::relay::Peer, pinc::CarrierKind) -> pinc::Result<()>,
    impl AsyncFn(&(), Vec<u8>) -> (pinc::Result<usize>, Vec<u8>),
    impl AsyncFn(&(), Vec<u8>, usize) -> (pinc::Result<usize>, Vec<u8>),
    impl AsyncFn(()),
    (),
> {
    pinc::relay::CarrierFns::new(
        async || -> pinc::Result<((), pinc::CarrierKind)> { std::future::pending().await },
        async |_: &pinc::relay::Peer, _: pinc::CarrierKind| -> pinc::Result<()> {
            Err(pinc::Error::new(
                pinc::ErrorKind::Unreachable,
                "this hub opens no carriers",
            ))
        },
        async |_: &(), buf: Vec<u8>| -> (pinc::Result<usize>, Vec<u8>) { (Ok(0), buf) },
        async |_: &(), buf: Vec<u8>, _: usize| -> (pinc::Result<usize>, Vec<u8>) { (Ok(0), buf) },
        async |_: ()| {},
    )
}

/// Announce the hub's ticket and run it until the edge closes.
async fn serve_hub<H, D>(
    built: pinc::Result<pinc::Endpoint<H, D>>,
    direct: Vec<SocketAddr>,
    ready: oneshot::Sender<Result<String>>,
    closing: edge::Edge,
) where
    H: Hosted + 'static,
    D: pinc::Discovery + 'static,
{
    let ep = match built {
        Ok(ep) => ep,
        Err(error) => {
            let _ = ready.send(Err(pinc_error("pinc hub", error)));
            return;
        }
    };
    let mut addr = ep.addr();
    addr.direct = direct;
    let relaying = ep.relay_stats().is_some();
    let _ = ready.send(Ok(addr.ticket()));
    let runner = ep.clone();
    // The hub's !Send endpoint lives on this pinned edge thread; its run
    // loop, which also drives the relay role, stays on it too.
    #[allow(clippy::disallowed_methods)]
    let run = nuclei::spawn_local(async move {
        if let Err(error) = runner.run().await {
            warn!(%error, "pinc hub failed");
        }
    });
    closing.closing().await;
    if let Some(stats) = ep.relay_stats() {
        info!(?stats, "pinc hub relay stopping");
    }
    ep.close().await;
    drop(run);
    debug!(relaying, "pinc hub stopped");
}
