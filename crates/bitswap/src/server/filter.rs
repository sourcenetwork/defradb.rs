//! Per-peer, per-cid request admission.

use std::future::Future;
use std::pin::Pin;

use cid::Cid;
use libp2p::PeerId;

type Verdict = Pin<Box<dyn Future<Output = bool> + Send + 'static>>;

/// Decides whether a peer may fetch a cid; resolves to true to serve it.
///
/// Async so the check can do I/O (policy or store lookups) without blocking the receive stage.
pub trait PeerBlockRequestFilter: Fn(&PeerId, &Cid) -> Verdict + Send + Sync + 'static {}

impl<F> PeerBlockRequestFilter for F where F: Fn(&PeerId, &Cid) -> Verdict + Send + Sync + 'static {}
