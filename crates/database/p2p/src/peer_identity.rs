//! Peer ACP identity resolution for serve-boundary checks.

use async_trait::async_trait;

use crate::transport::PeerId;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PeerIdentityResolver: Send + Sync {
    async fn resolve(&self, peer_id: &PeerId) -> Option<identity::Did>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnonymousResolver;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PeerIdentityResolver for AnonymousResolver {
    async fn resolve(&self, _peer_id: &PeerId) -> Option<identity::Did> {
        None
    }
}
