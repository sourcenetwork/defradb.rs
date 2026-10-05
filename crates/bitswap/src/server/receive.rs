//! The serial receive stage: classifies each inbound message and looks up block sizes before the engine sees it.
//!
//! Messages are handled strictly in arrival order, one at a time, so a slow filter or store delays later
//! messages rather than reordering them.

use cid::Cid;
use kovan_channel::{bounded, unbounded};
use libp2p::PeerId;
use tracing::debug;

use super::engine::{Command, Received, Want};
use super::filter::PeerBlockRequestFilter;
use crate::message::BitswapMessage;
use crate::store::Store;

pub(crate) type Inbound = (PeerId, BitswapMessage);

pub(crate) async fn run<S: Store>(
    inbound: bounded::Receiver<Inbound>,
    commands: unbounded::Sender<Command>,
    store: S,
    filter: Option<Box<dyn PeerBlockRequestFilter>>,
    max_wants: usize,
) {
    while let Some((peer, message)) = inbound.recv_async().await {
        if message.is_empty() {
            debug!(%peer, "received empty message");
        }
        let received = prepare(peer, &message, &store, filter.as_deref(), max_wants).await;
        commands.send(Command::Received(received));
    }
}

async fn prepare<S: Store>(
    peer: PeerId,
    message: &BitswapMessage,
    store: &S,
    filter: Option<&dyn PeerBlockRequestFilter>,
    max_wants: usize,
) -> Received {
    let mut cancels: Vec<Cid> = Vec::new();
    let mut denials = Vec::new();
    let mut allowed = Vec::new();

    for entry in message.wantlist() {
        if entry.cancel {
            cancels.push(entry.cid);
            continue;
        }
        let permitted = match filter {
            Some(filter) => filter(&peer, &entry.cid).await,
            None => true,
        };
        if !permitted {
            denials.push(entry.clone());
        } else if max_wants == 0 || allowed.len() < max_wants {
            allowed.push(entry);
        }
    }

    let mut wants = Vec::with_capacity(allowed.len());
    for entry in allowed {
        let size = store.get_size(&entry.cid).await.ok();
        wants.push(Want {
            entry: entry.clone(),
            size,
        });
    }

    Received {
        peer,
        full: message.full(),
        cancels,
        denials,
        wants,
    }
}
