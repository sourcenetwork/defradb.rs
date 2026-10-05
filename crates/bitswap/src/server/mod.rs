//! The bitswap server: answers wants from a block store with the reference decision rules.
//!
//! A bounded inbound queue feeds a serial receive stage, which feeds the engine owner task. Both tasks end
//! when the [`Server`] is dropped.

mod config;
mod engine;
mod envelope;
mod filter;
pub mod ledger;
pub mod peer_task_queue;
mod receive;
pub mod task_merger;
pub mod wantlist;

use kovan_channel::{bounded, unbounded};
use libp2p::PeerId;
use thiserror::Error;
use tokio::sync::oneshot;

pub use config::ServerConfig;
pub use filter::PeerBlockRequestFilter;

use engine::{Command, Engine};
use receive::Inbound;

use crate::message::BitswapMessage;
use crate::network::Network;
use crate::store::Store;

const INBOUND_CAPACITY: usize = 2048;

/// The inbound queue was full, so the message was dropped.
#[derive(Debug, Error)]
#[error("server inbound queue is full")]
pub struct InboundFull;

/// Handle to the server tasks.
pub struct Server {
    inbound: bounded::Sender<Inbound>,
    commands: unbounded::Sender<Command>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

impl Server {
    /// Spawns the server tasks on the current tokio runtime.
    pub fn new<S: Store>(network: Network, store: S, config: ServerConfig) -> Self {
        let (inbound, inbound_rx) = kovan_channel::bounded(INBOUND_CAPACITY);
        let (commands, commands_rx) = kovan_channel::unbounded();
        let (reports, reports_rx) = kovan_channel::unbounded();

        let max_wants = config.max_queued_wantlist_entries_per_peer;
        let engine = Engine::new(&config, store.clone(), network, reports);
        tokio::spawn(receive::run(
            inbound_rx,
            commands.clone(),
            store,
            config.peer_block_request_filter,
            max_wants,
        ));
        tokio::spawn(engine.run(commands_rx, reports_rx));

        Server { inbound, commands }
    }

    /// Queues an inbound message for processing without waiting.
    ///
    /// `&mut self` keeps this the only sender, so the capacity check cannot race into a blocking send.
    pub fn try_receive_message(
        &mut self,
        peer: PeerId,
        message: BitswapMessage,
    ) -> Result<(), InboundFull> {
        if self.inbound.is_full() {
            return Err(InboundFull);
        }
        self.inbound.send((peer, message));
        Ok(())
    }

    /// A peer became reachable over bitswap.
    pub fn peer_connected(&self, peer: PeerId) {
        self.commands.send(Command::PeerConnected(peer));
    }

    /// Wants recorded for the peer, or `None` when it holds no ledger.
    pub async fn ledger_wants(&self, peer: PeerId) -> Option<usize> {
        let (reply, wants) = oneshot::channel();
        self.commands.send(Command::LedgerWants(peer, reply));
        wants.await.ok().flatten()
    }

    /// A peer is gone; its wantlist is dropped.
    pub fn peer_disconnected(&self, peer: PeerId) {
        self.commands.send(Command::PeerDisconnected(peer));
    }
}
