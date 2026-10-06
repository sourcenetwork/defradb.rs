//! The serial receive stage: classifies each inbound message and looks up block sizes before the engine sees it.
//!
//! Messages are handled strictly in arrival order, one at a time. Each message has one lookup deadline, so a
//! hung filter or store costs it at most that long: unanswered checks fall back to denied and sizes to absent.

use std::future::Future;
use std::time::Duration;

use cid::Cid;
use kovan_channel::{bounded, unbounded};
use libp2p::PeerId;
use tokio::time::{timeout_at, Instant};
use tracing::{debug, warn};

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
    lookup_timeout: Duration,
) {
    while let Some((peer, message)) = inbound.recv_async().await {
        if message.is_empty() {
            debug!(%peer, "received empty message");
        }
        let received = prepare(
            peer,
            &message,
            &store,
            filter.as_deref(),
            max_wants,
            lookup_timeout,
        )
        .await;
        commands.send(Command::Received(received));
    }
}

async fn prepare<S: Store>(
    peer: PeerId,
    message: &BitswapMessage,
    store: &S,
    filter: Option<&dyn PeerBlockRequestFilter>,
    max_wants: usize,
    lookup_timeout: Duration,
) -> Received {
    // A timeout too large to represent as an instant means no deadline rather than a panic.
    let deadline = Instant::now().checked_add(lookup_timeout);
    let mut fallbacks = 0usize;
    let mut cancels: Vec<Cid> = Vec::new();
    let mut denials = Vec::new();
    let mut allowed = Vec::new();

    for entry in message.wantlist() {
        if entry.cancel {
            cancels.push(entry.cid);
            continue;
        }
        let permitted = match filter {
            Some(filter) => match before(deadline, filter(&peer, &entry.cid)).await {
                Some(verdict) => verdict,
                None => {
                    fallbacks += 1;
                    false
                }
            },
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
        let size = match before(deadline, store.get_size(&entry.cid)).await {
            Some(result) => result.ok(),
            None => {
                fallbacks += 1;
                None
            }
        };
        wants.push(Want {
            entry: entry.clone(),
            size,
        });
    }

    if fallbacks > 0 {
        warn!(%peer, fallbacks, "message lookups hit the deadline");
    }

    Received {
        peer,
        full: message.full(),
        cancels,
        denials,
        wants,
    }
}

async fn before<T>(deadline: Option<Instant>, lookup: impl Future<Output = T>) -> Option<T> {
    let Some(deadline) = deadline else {
        return Some(lookup.await);
    };
    if Instant::now() >= deadline {
        return None;
    }
    timeout_at(deadline, lookup).await.ok()
}
