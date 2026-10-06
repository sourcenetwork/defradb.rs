#![allow(dead_code)]

use std::future::poll_fn;
use std::time::Duration;

use bitswap::network::{Network, OutEvent, SendError};
use bitswap::server::{Server, ServerConfig};
use bitswap::{BitswapMessage, Block, WantType};
use cid::Cid;
use libp2p::PeerId;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::common::mem_store::MemStore;

pub struct Sent {
    pub message: BitswapMessage,
    pub response: oneshot::Sender<Result<(), SendError>>,
}

pub struct Harness {
    pub server: Server,
    pub sent: mpsc::UnboundedReceiver<Sent>,
    pub peer: PeerId,
}

pub fn start(blocks: &[Block], config: ServerConfig) -> Harness {
    start_with(MemStore::new(blocks), config)
}

pub fn start_with<S: bitswap::Store>(store: S, config: ServerConfig) -> Harness {
    let (network, mut events) = Network::new(PeerId::random());
    let (tx, sent) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match poll_fn(|cx| events.poll_next(cx)).await {
                OutEvent::Dial { response, .. } => {
                    let _ = response.send(Ok(None));
                }
                OutEvent::SendMessage {
                    message, response, ..
                } => {
                    let _ = tx.send(Sent { message, response });
                }
                OutEvent::Protect { .. } | OutEvent::Unprotect { .. } => {}
            }
        }
    });
    let server = Server::new(network, store, config);
    let peer = PeerId::random();
    server.peer_connected(peer);
    Harness { server, sent, peer }
}

impl Harness {
    pub fn send(&mut self, message: BitswapMessage) {
        self.server
            .try_receive_message(self.peer, message)
            .expect("inbound queue has room");
    }

    pub async fn next(&mut self) -> Sent {
        timeout(Duration::from_secs(5), self.sent.recv())
            .await
            .expect("a message within the deadline")
            .expect("the capture task is alive")
    }

    /// Collects messages, acknowledging each, until one carries the sentinel block.
    pub async fn until_sentinel(&mut self, sentinel: &Cid) -> Vec<BitswapMessage> {
        let mut out = Vec::new();
        loop {
            let sent = self.next().await;
            let _ = sent.response.send(Ok(()));
            let done = sent.message.blocks().any(|b| b.cid == *sentinel);
            out.push(sent.message);
            if done {
                return out;
            }
        }
    }
}

pub fn want(cid: Cid, want_type: WantType, send_dont_have: bool) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_entry(cid, 1, want_type, send_dont_have);
    message
}

pub fn cancel(cid: Cid) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.cancel(cid);
    message
}

pub fn has_block(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.blocks().any(|b| b.cid == *cid))
}

pub fn has_dont_have(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.dont_haves().any(|c| c == cid))
}

pub fn has_have(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.haves().any(|c| c == cid))
}

impl Harness {
    /// Lets every queued tick run, then acknowledges and returns every message the server sent.
    pub async fn drain(&mut self) -> Vec<BitswapMessage> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut out = Vec::new();
        while let Ok(sent) = self.sent.try_recv() {
            let _ = sent.response.send(Ok(()));
            out.push(sent.message);
        }
        out
    }
}

/// A want with an explicit priority.
pub fn want_with(cid: Cid, priority: i32, send_dont_have: bool) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_entry(cid, priority, WantType::Block, send_dont_have);
    message
}
