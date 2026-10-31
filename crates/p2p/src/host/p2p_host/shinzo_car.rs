//! Go Shinzo's CAR channel in the host: serving a Go `CARRequest` from the
//! block store under the same access gate Bitswap applies, and asking a peer
//! for a head's CAR in Go's shape. On only when `P2PHostConfig::shinzo_wire`
//! is set; otherwise a Go request is ignored and CAR requests stay Rust's.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ::bitswap::Store;
use cid::Cid;
use libp2p::PeerId;
use tracing::debug;

use super::P2PHost;
use crate::error::{Error, Result};
use crate::message::car_exchange::{CarReply, CarRequest};
use crate::sync::shinzo_car::{collect_commit_blocks, serve_cars, HeadCar};

/// Whether a peer may read a block: Bitswap's per-peer egress filter.
pub(crate) type BlockAccess =
    Arc<dyn Fn(&PeerId, &Cid) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// What the host needs to serve Go CAR requests.
pub(crate) struct ShinzoCars<S> {
    store: S,
    access: BlockAccess,
}

impl<S: Clone> Clone for ShinzoCars<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            access: Arc::clone(&self.access),
        }
    }
}

impl<S: Store> ShinzoCars<S> {
    pub(crate) fn new(store: S, access: BlockAccess) -> Self {
        Self { store, access }
    }

    /// The CAR of one commit rooted at `head`, or a skip: not held, not
    /// readable by `peer`, not a document block, or missing a block.
    async fn car_for(&self, peer: PeerId, head: Cid) -> HeadCar {
        if !self.store.has(&head).await.unwrap_or(false) || !(self.access)(&peer, &head).await {
            return HeadCar::Skip;
        }
        let store = &self.store;
        let blocks = collect_commit_blocks(head, |cid| async move {
            match store.has(&cid).await {
                Ok(true) => store
                    .get(&cid)
                    .await
                    .map(|block| Some(block.data))
                    .map_err(|error| Error::BlockstoreError(error.0)),
                Ok(false) => Ok(None),
                Err(error) => Err(Error::BlockstoreError(error.0)),
            }
        })
        .await;
        let Ok(Some(blocks)) = blocks else {
            return HeadCar::Skip;
        };
        let refs: Vec<(&Cid, &[u8])> = blocks
            .iter()
            .map(|(cid, data)| (cid, data.as_ref()))
            .collect();
        crate::sync::car::encode_car_response(&[head], &refs, &[])
            .map_or(HeadCar::Skip, HeadCar::Car)
    }
}

impl<S: Store> P2PHost<S> {
    /// Answer a Go CAR request, if the Shinzo wire is on.
    pub(super) fn serve_shinzo_cars(&mut self, peer_id: PeerId, request: CarRequest) {
        let Some(cars) = self.shinzo.clone() else {
            debug!(%peer_id, "Ignoring a Go Shinzo CAR request: the Shinzo wire is off");
            return;
        };
        let keypair = self.keypair.clone();
        let mut handler = self.two_stream_handler.clone();
        self.spawned_tasks.spawn(async move {
            let heads: Vec<Vec<u8>> = request.cid_bytes().map(<[u8]>::to_vec).collect();
            let asked: Vec<&[u8]> = heads.iter().map(Vec::as_slice).collect();
            let entries = serve_cars(&asked, |head| cars.car_for(peer_id, head)).await;
            let mut reply = CarReply::new(&request.message_id, entries);
            if let Err(error) = crate::signing::sign_message(&keypair, &mut reply) {
                debug!(%peer_id, %error, "Failed to sign a Go CAR reply");
                return;
            }
            if let Err(error) = handler.send_shinzo_car_reply(peer_id, &reply).await {
                debug!(%peer_id, %error, "Failed to send a Go CAR reply");
            }
        });
    }

    /// Ask `peer_id` for `root`'s CAR in Go's shape. Its reply, or a CAR with
    /// no blocks when the peer would not serve it, arrives as the usual CAR
    /// fetch response.
    pub(crate) fn send_shinzo_car_request(
        &mut self,
        peer_id: PeerId,
        root: Cid,
        response: tokio::sync::oneshot::Sender<Result<()>>,
    ) {
        let mut request = CarRequest::new(&[root]);
        if let Err(error) = crate::signing::sign_message(&self.keypair, &mut request) {
            let _ = response.send(Err(error));
            return;
        }
        let mut handler = self.two_stream_handler.clone();
        self.spawned_tasks.spawn(async move {
            let result = handler
                .send_shinzo_car_request(peer_id, root, &request)
                .await;
            if response.send(result).is_err() {
                debug!(%peer_id, "SendCarRequest command response dropped");
            }
        });
    }
}
