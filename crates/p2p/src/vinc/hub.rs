//! Hubs: what vinc topic broadcast runs through.
//!
//! A node names its hubs (vinc's relays) when its endpoint is built;
//! subscriptions and publishes go to them, and they push each message to the
//! topic's members. [`spawn_hub`] runs one with no node on top, for a test
//! process or an operator.

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::rt::oneshot;

use super::addr::parse_node_addr;
use super::endpoint::NucleiIo;
use super::services::ALPN_MUX;
use super::wire::vinc_error;

/// vinc's bound on the relays an endpoint names.
const MAX_HUBS: usize = 4;

/// The endpoint's sizes: `connections` sessions, and room for a topic per
/// document (vinc's ceiling). The topic message limit stays vinc's 64 KiB,
/// libp2p gossipsub's default too; iroh's is 4 KiB.
pub(super) fn limits(connections: usize) -> vinc::Limits {
    vinc::Limits {
        connections,
        topics: 4096,
        ..vinc::Limits::default()
    }
}

/// The hubs `tickets` name, each `<peer id>@<udp address>`.
pub(super) fn hubs(tickets: &[String]) -> Result<Vec<vinc::Relay>> {
    if tickets.len() > MAX_HUBS {
        return Err(Error::InvalidConfig(format!(
            "a vinc node names at most {MAX_HUBS} hubs, not {}",
            tickets.len()
        )));
    }
    tickets
        .iter()
        .map(|ticket| {
            let addr = parse_node_addr(ticket)?;
            let udp = addr.direct.first().copied().ok_or_else(|| {
                Error::InvalidConfig(format!("vinc hub {ticket} has no UDP address"))
            })?;
            Ok(vinc::Relay::new(addr.node).with_udp(udp))
        })
        .collect()
}

/// Run a hub on a thread of `edge`, bound to `bind`, under the identity
/// `seed`. Returns its ticket for [`VincEndpointConfig::hubs`](super::VincEndpointConfig)
/// and the task, which runs until the edge closes.
pub async fn spawn_hub(
    seed: [u8; 32],
    bind: SocketAddr,
    edge: edge::Edge,
) -> Result<(String, edge::Task<()>)> {
    let (ready_tx, ready_rx) = oneshot::channel::<Result<String>>();
    let closing = edge.clone();
    let task = edge
        .spawn_pinned(move || async move {
            let socket = match nuclei::Handle::<std::net::UdpSocket>::bind(bind).await {
                Ok(socket) => socket,
                Err(error) => {
                    let _ = ready_tx.send(Err(Error::Transport(format!(
                        "failed to bind vinc hub socket {bind}: {error}"
                    ))));
                    return;
                }
            };
            let local = socket.local_addr().unwrap_or(bind);
            let built = vinc::Vinc::builder()
                .seed(&seed)
                .and_then(|builder| {
                    builder
                        .alpn(ALPN_MUX)
                        .local_addr(local)
                        .limits(limits(vinc::Limits::default().connections))
                        .hub()
                        .build_io(NucleiIo {
                            socket: Rc::new(socket),
                        })
                })
                .map_err(|error| vinc_error("vinc hub", error));
            let ep = match built {
                Ok(ep) => ep,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(ep.addr().ticket()));
            let runner = ep.clone();
            let run = nuclei::spawn_local(async move {
                if let Err(error) = runner.run().await {
                    warn!(%error, "vinc hub failed");
                }
            });
            closing.closing().await;
            ep.close().await;
            drop(run);
            debug!("vinc hub stopped");
        })
        .within(Duration::MAX);
    let ticket = ready_rx
        .await
        .map_err(|_| Error::Transport("the vinc hub stopped before it was bound".to_owned()))??;
    Ok((ticket, task))
}
