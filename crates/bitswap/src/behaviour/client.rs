//! The fetch API of the behaviour and the poll step that drives the client.

use std::task::Context;

use cid::Cid;
use libp2p::PeerId;
use tokio::sync::mpsc;

use super::Bitswap;
use crate::block::Block;
use crate::client::FetchId;
use crate::peer_state::PeerState;
use crate::store::Store;

impl<S: Store> Bitswap<S> {
    /// Fetches `cids` from `providers`. Each block is delivered once on the receiver, which closes when
    /// every cid is resolved; cids no provider has are skipped. Empty cids or providers close it at once.
    pub fn fetch(
        &mut self,
        cids: Vec<Cid>,
        providers: Vec<PeerId>,
    ) -> (FetchId, mpsc::Receiver<Block>) {
        let fetch = self.client.fetch(cids, providers);
        self.driver.wake();
        fetch
    }

    /// Cancels a fetch and its outstanding requests; true when it was live.
    pub fn cancel(&mut self, id: FetchId) -> bool {
        let live = self.client.cancel(id);
        self.driver.wake();
        live
    }

    pub(super) fn poll_client(&mut self, cx: &mut Context<'_>) {
        let peers = &self.peers;
        self.driver.poll(
            &mut self.client,
            |peer| match peers.get(peer) {
                Some(PeerState::Responsive(_, protocol)) => Some(*protocol),
                _ => None,
            },
            cx,
        );
    }
}
