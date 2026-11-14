//! `P2PTransport` over a vinc endpoint.
//!
//! `VincTransport` is the `Clone + Send + Sync` facade: identity and signing
//! are local, replicators live in the shared registry, and everything that
//! touches the network is a command to the endpoint's thread.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cid::Cid;
use ed25519_dalek::Signer as _;
use futures::FutureExt;

use crate::bitswap::ReplicatorRegistry;
use crate::error::{Error, Result};
use crate::explicit_replay::ExplicitReplayCapabilityCache;
use crate::message::{
    BranchableSyncReply, BranchableSyncRequest, CarFetchRequest, DocSyncReply, DocSyncRequest,
    ManageQueryReply, ManageQueryRequest, ManageReply, ManageRequest, PushLogBroadcast,
    PushLogReply, PushLogRequest, PushSEArtifactsRequest, QuerySEArtifactsReply,
    QuerySEArtifactsRequest,
};
use crate::replicator::ReplicatorInfo;
use crate::rt::{mpsc, oneshot};
use crate::signing::sign_with_transport;
use crate::topics::DefraTopic;
use crate::transport::{MessageId, P2PTransport, PeerAddr, PeerId};
use crate::QueryId;

use super::admission::AdmissionAuthority;
use super::command::{Command, CommandSender, EndpointStopped, Outbound, ReplyBody};
use super::services;
use super::token::VincResponseToken;
use super::wire;

#[derive(Clone)]
pub struct VincTransport {
    // Dropped before `commands`: the endpoint, woken by the last sender going,
    // must already see that no transport is left.
    stopped: EndpointStopped,
    commands: mpsc::Sender<Command>,
    local_peer_id: PeerId,
    public_key: Vec<u8>,
    signing_key: Arc<ed25519_dalek::SigningKey>,
    replicators: Arc<ReplicatorRegistry>,
    explicit_replay_capabilities: ExplicitReplayCapabilityCache,
}

impl VincTransport {
    pub(super) fn new(
        commands: CommandSender,
        seed: &[u8; 32],
        replicators: Arc<ReplicatorRegistry>,
    ) -> Self {
        let (commands, stopped) = commands;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(seed);
        let public_key = signing_key.verifying_key().to_bytes().to_vec();
        Self {
            stopped,
            commands,
            local_peer_id: PeerId::new(hex::encode(&public_key)),
            public_key,
            signing_key: Arc::new(signing_key),
            replicators,
            explicit_replay_capabilities: ExplicitReplayCapabilityCache::default(),
        }
    }

    async fn send_command<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T>>) -> Command,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(make(tx))
            .await
            .map_err(|_| Error::ChannelSend)?;
        rx.await.map_err(|_| Error::ChannelReceive)?
    }

    async fn request(
        &self,
        peer_id: &PeerId,
        tag: &'static [u8],
        body: Vec<u8>,
        outbound: Outbound,
    ) -> Result<Vec<u8>> {
        self.send_command(|reply| Command::Request {
            peer_id: peer_id.clone(),
            tag,
            body,
            outbound,
            reply,
        })
        .await
    }

    async fn reply(&self, token: VincResponseToken, body: ReplyBody) -> Result<()> {
        let (id, commands) = token.spend();
        let (tx, rx) = oneshot::channel();
        commands
            .send(Command::Reply {
                id,
                body,
                reply: tx,
            })
            .await
            .map_err(|_| Error::ChannelSend)?;
        rx.await.map_err(|_| Error::ChannelReceive)?
    }

    async fn reply_message<T>(&self, token: VincResponseToken, reply: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        self.reply(token, ReplyBody::Framed(wire::encode(reply)?))
            .await
    }

    pub fn set_explicit_replay_capability(
        &self,
        peer_id: &PeerId,
        collection_id: &str,
        capability: &str,
    ) -> Result<()> {
        self.explicit_replay_capabilities.set(
            self.local_peer_id.as_str(),
            peer_id.as_str(),
            collection_id,
            capability,
        )
    }

    pub fn clear_explicit_replay_capabilities(&self, peer_id: &PeerId, collections: &[String]) {
        self.explicit_replay_capabilities
            .clear(peer_id.as_str(), collections);
    }

    pub fn explicit_replay_capability_matches(
        &self,
        peer_id: &PeerId,
        collection_id: &str,
        capability: Option<&str>,
    ) -> bool {
        self.explicit_replay_capabilities
            .matches(peer_id.as_str(), collection_id, capability)
    }

    /// Tell the endpoint the network may have changed: it probes its paths.
    pub async fn network_change(&self) -> Result<()> {
        self.send_command(|reply| Command::NetworkChange { reply })
            .await
    }

    /// Admit `peer_id`: add it to an explicit allowlist and, with the
    /// authority to revoke, lift a revocation of it.
    pub async fn allow_peer(&self, peer_id: &PeerId, authority: AdmissionAuthority) -> Result<()> {
        self.send_command(|reply| Command::AllowPeer {
            peer_id: peer_id.clone(),
            authority,
            reply,
        })
        .await
    }

    /// Bar `peer_id` in both directions: every session with it ends within
    /// the call, and no new one comes up until it is allowed again.
    pub async fn deny_peer(&self, peer_id: &PeerId) -> Result<()> {
        self.send_command(|reply| Command::DenyPeer {
            peer_id: peer_id.clone(),
            reply,
        })
        .await
    }

    pub async fn is_peer_revoked(&self, peer_id: &PeerId) -> Result<bool> {
        self.send_command(|reply| Command::IsPeerRevoked {
            peer_id: peer_id.clone(),
            reply,
        })
        .await
    }

    /// Publish `pointer` (the service-record pointer) in this node's record,
    /// and hand the record to every connected peer.
    pub async fn set_service_pointer(
        &self,
        pointer: [u8; services::SERVICE_POINTER_LEN],
    ) -> Result<()> {
        self.send_command(|reply| Command::SetServicePointer { pointer, reply })
            .await
    }

    /// The service-record pointer `peer_id`'s record carries, if this node
    /// has its record and it carries one.
    pub async fn service_pointer(&self, peer_id: &PeerId) -> Result<Option<Vec<u8>>> {
        self.send_command(|reply| Command::ServicePointer {
            peer_id: peer_id.clone(),
            reply,
        })
        .await
    }

    /// Resolve a peer's Defra identity: it signs a token whose audience is
    /// this node, over the connection vinc authenticated.
    pub async fn get_peer_identity(&self, peer_id: &PeerId) -> Result<Option<identity::Did>> {
        let request = crate::message::IdentityRequest::new(self.local_peer_id.to_string());
        let bytes = self
            .request(
                peer_id,
                services::STREAM_IDENTITY,
                wire::encode(&request)?,
                Outbound::Ask,
            )
            .await?;
        let response: crate::message::IdentityResponse = wire::decode(bytes)?;
        if let Some(error) = response.err_message.as_deref() {
            if error == crate::message::IDENTITY_UNCONFIGURED_ERROR {
                return Ok(None);
            }
            return Err(Error::Transport(format!(
                "peer identity challenge failed: {error}"
            )));
        }
        let token_identity = identity::from_token(&response.identity_token)
            .map_err(|error| Error::Transport(format!("invalid peer identity token: {error}")))?;
        identity::verify_auth_token(&token_identity, self.local_peer_id.as_str()).map_err(
            |error| Error::Transport(format!("peer identity token verification failed: {error}")),
        )?;
        let did = identity::Identity::did(&token_identity)
            .map_err(|error| Error::Transport(format!("peer identity DID failed: {error}")))?;
        Ok(Some(did))
    }
}

#[async_trait]
impl P2PTransport for VincTransport {
    type ResponseToken = VincResponseToken;

    fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    fn local_public_key_proto(&self) -> &[u8] {
        &self.public_key
    }

    fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(self.signing_key.sign(data).to_bytes().to_vec())
    }

    async fn dial(&self, peer_id: &PeerId, addrs: Vec<PeerAddr>) -> Result<()> {
        self.send_command(|reply| Command::Dial {
            peer_id: peer_id.clone(),
            addrs,
            reply,
        })
        .await
    }

    async fn disconnect(&self, peer_id: &PeerId) -> Result<()> {
        self.send_command(|reply| Command::Disconnect {
            peer_id: peer_id.clone(),
            reply,
        })
        .await
    }

    fn parse_dial_addr(&self, addr: &str) -> Result<(PeerId, Vec<PeerAddr>)> {
        super::addr::parse_public_peer_addr(addr)
    }

    async fn listen(&self, _addr: PeerAddr) -> Result<()> {
        // The endpoint listens on the address it was bound to.
        Ok(())
    }

    async fn connected_peers(&self) -> Result<Vec<PeerId>> {
        self.send_command(|reply| Command::ConnectedPeers { reply })
            .await
    }

    async fn inbound_budget(&self) -> Result<Vec<crate::transport::InboundClassSnapshot>> {
        self.send_command(|reply| Command::InboundBudget { reply })
            .await
    }

    async fn peer_connection(
        &self,
        peer_id: &PeerId,
    ) -> Result<Option<crate::transport::PeerConnectionInfo>> {
        self.send_command(|reply| Command::PeerConnection {
            peer_id: peer_id.clone(),
            reply,
        })
        .await
    }

    async fn listen_addresses(&self) -> Result<Vec<PeerAddr>> {
        self.send_command(|reply| Command::ListenAddresses { reply })
            .await
    }

    async fn poll_until_connected(&self, peer_id: &PeerId, timeout: Duration) -> Result<()> {
        let wait_for_peer = self.send_command(|reply| Command::WaitConnected {
            peer_id: peer_id.clone(),
            reply,
        });
        futures::select_biased! {
            _ = self.stopped.clone() => Err(Error::ChannelSend),
            result = crate::rt::time::timeout(timeout, wait_for_peer).fuse() => {
                result.unwrap_or_else(|_| Err(Error::ConnectionTimeout(peer_id.to_string())))
            }
        }
    }

    async fn peer_addresses(&self) -> Result<Vec<String>> {
        self.send_command(|reply| Command::PeerAddresses { reply })
            .await
    }

    async fn subscribe(&self, topic: DefraTopic) -> Result<bool> {
        self.send_command(|reply| Command::Subscribe {
            topic: topic.to_string(),
            raw: false,
            reply,
        })
        .await
    }

    async fn unsubscribe(&self, topic: DefraTopic) -> Result<bool> {
        self.send_command(|reply| Command::Unsubscribe {
            topic: topic.to_string(),
            reply,
        })
        .await
    }

    async fn publish(&self, topic: DefraTopic, msg: PushLogBroadcast) -> Result<MessageId> {
        let payload = msg
            .encode_gossip_payload()
            .map_err(|error| Error::CborSerialization(error.to_string()))?;
        self.send_command(|reply| Command::Publish {
            topic: topic.to_string(),
            payload,
            wait: false,
            reply,
        })
        .await
    }

    async fn publish_raw(&self, topic: String, data: Vec<u8>) -> Result<MessageId> {
        // A KMS request or reply: published before any member is known, it
        // reaches nobody.
        self.send_command(|reply| Command::Publish {
            topic,
            payload: data,
            wait: true,
            reply,
        })
        .await
    }

    async fn subscribe_raw(&self, topic: String) -> Result<bool> {
        self.send_command(|reply| Command::Subscribe {
            topic,
            raw: true,
            reply,
        })
        .await
    }

    async fn register_pubsub_rpc_topic(&self, topic: String) -> Result<()> {
        self.send_command(|reply| Command::RegisterRawTopic { topic, reply })
            .await
    }

    async fn topic_peers(&self, topic: DefraTopic) -> Result<Vec<PeerId>> {
        self.send_command(|reply| Command::TopicPeers {
            topic: topic.to_string(),
            reply,
        })
        .await
    }

    async fn send_pushlog_response(
        &self,
        token: Self::ResponseToken,
        reply: PushLogReply,
    ) -> Result<()> {
        self.reply_message(token, &reply).await
    }

    async fn send_two_stream_request(
        &self,
        peer_id: &PeerId,
        mut req: PushLogRequest,
    ) -> Result<PushLogReply> {
        self.explicit_replay_capabilities
            .attach(peer_id.as_str(), &mut req);
        // The ack comes back on the request's stream; the capability is part
        // of the wire message, so it is signed again.
        req.supports_same_stream_reply = true;
        req.supports_retry_after = true;
        sign_with_transport(self, &mut req)?;
        let bytes = self
            .request(
                peer_id,
                services::STREAM_TWOSTREAM,
                wire::encode(&req)?,
                Outbound::Ask,
            )
            .await?;
        wire::decode(bytes)
    }

    async fn send_two_stream_response(
        &self,
        _peer_id: &PeerId,
        _reply: PushLogReply,
    ) -> Result<()> {
        Err(no_reply_stream("two-stream"))
    }

    async fn send_doc_sync_request(&self, peer_id: &PeerId, req: DocSyncRequest) -> Result<()> {
        self.request(
            peer_id,
            services::STREAM_DOCSYNC,
            wire::encode(&req)?,
            Outbound::Event {
                to_event: |peer_id, bytes| {
                    Ok(crate::transport::TransportEvent::DocSyncReply {
                        peer_id,
                        reply: wire::decode(bytes)?,
                    })
                },
                answer_when_sent: false,
            },
        )
        .await
        .map(drop)
    }

    async fn send_doc_sync_response(&self, _peer_id: &PeerId, _reply: DocSyncReply) -> Result<()> {
        Err(no_reply_stream("doc sync"))
    }

    async fn send_branchable_sync_request(
        &self,
        peer_id: &PeerId,
        req: BranchableSyncRequest,
    ) -> Result<()> {
        self.request(
            peer_id,
            services::STREAM_BRANCHABLE,
            wire::encode(&req)?,
            Outbound::Event {
                to_event: |peer_id, bytes| {
                    Ok(crate::transport::TransportEvent::BranchableSyncReply {
                        peer_id,
                        reply: wire::decode(bytes)?,
                    })
                },
                answer_when_sent: false,
            },
        )
        .await
        .map(drop)
    }

    async fn send_branchable_sync_response(
        &self,
        _peer_id: &PeerId,
        _reply: BranchableSyncReply,
    ) -> Result<()> {
        Err(no_reply_stream("branchable sync"))
    }

    async fn send_car_request(&self, peer_id: &PeerId, root_cid: Cid) -> Result<()> {
        self.send_command(|reply| Command::SendCarRequest {
            peer_id: peer_id.clone(),
            root_cid,
            reply,
        })
        .await
    }

    async fn send_car_response(&self, _peer_id: &PeerId, _car_data: Vec<u8>) -> Result<()> {
        Err(no_reply_stream("CAR"))
    }

    async fn send_car_response_token(
        &self,
        token: Self::ResponseToken,
        car_data: Vec<u8>,
    ) -> Result<()> {
        self.reply(token, ReplyBody::Raw(car_data)).await
    }

    async fn send_doc_sync_response_token(
        &self,
        token: Self::ResponseToken,
        reply: DocSyncReply,
    ) -> Result<()> {
        self.reply_message(token, &reply).await
    }

    async fn send_branchable_sync_response_token(
        &self,
        token: Self::ResponseToken,
        reply: BranchableSyncReply,
    ) -> Result<()> {
        self.reply_message(token, &reply).await
    }

    async fn send_se_artifacts(
        &self,
        peer_id: &PeerId,
        mut req: PushSEArtifactsRequest,
    ) -> Result<()> {
        sign_with_transport(self, &mut req)?;
        self.request(
            peer_id,
            services::STREAM_SE,
            wire::encode(&req)?,
            Outbound::Tell,
        )
        .await
        .map(drop)
    }

    async fn send_se_query_request(
        &self,
        peer_id: &PeerId,
        req: QuerySEArtifactsRequest,
    ) -> Result<()> {
        self.request(
            peer_id,
            services::STREAM_SE_QUERY,
            wire::encode(&req)?,
            Outbound::Event {
                to_event: |peer_id, bytes| {
                    let reply: QuerySEArtifactsReply = wire::decode(bytes)?;
                    super::serve::verify_signed_sender(&peer_id, &reply)?;
                    Ok(crate::transport::TransportEvent::SEQueryReply { peer_id, reply })
                },
                answer_when_sent: true,
            },
        )
        .await
        .map(drop)
    }

    async fn send_se_query_response(
        &self,
        _peer_id: &PeerId,
        token: Option<Self::ResponseToken>,
        reply: QuerySEArtifactsReply,
    ) -> Result<()> {
        match token {
            Some(token) => self.reply_message(token, &reply).await,
            None => Err(no_reply_stream("searchable-encryption query")),
        }
    }

    async fn send_manage_request(&self, peer_id: &PeerId, req: ManageRequest) -> Result<()> {
        self.request(
            peer_id,
            services::STREAM_MANAGE,
            wire::encode(&req)?,
            Outbound::Event {
                to_event: |peer_id, bytes| {
                    let reply: ManageReply = wire::decode(bytes)?;
                    super::serve::verify_signed_sender(&peer_id, &reply)?;
                    Ok(crate::transport::TransportEvent::ManageReply { peer_id, reply })
                },
                answer_when_sent: true,
            },
        )
        .await
        .map(drop)
    }

    async fn send_manage_response(
        &self,
        _peer_id: &PeerId,
        token: Option<Self::ResponseToken>,
        reply: ManageReply,
    ) -> Result<()> {
        match token {
            Some(token) => self.reply_message(token, &reply).await,
            None => Err(no_reply_stream("management")),
        }
    }

    async fn send_manage_query_request(
        &self,
        peer_id: &PeerId,
        req: ManageQueryRequest,
    ) -> Result<()> {
        self.request(
            peer_id,
            services::STREAM_MANAGE_QUERY,
            wire::encode(&req)?,
            Outbound::Event {
                to_event: |peer_id, bytes| {
                    let reply: ManageQueryReply = wire::decode(bytes)?;
                    super::serve::verify_signed_sender(&peer_id, &reply)?;
                    Ok(crate::transport::TransportEvent::ManageQueryReply { peer_id, reply })
                },
                answer_when_sent: true,
            },
        )
        .await
        .map(drop)
    }

    async fn send_manage_query_response(
        &self,
        _peer_id: &PeerId,
        token: Option<Self::ResponseToken>,
        reply: ManageQueryReply,
    ) -> Result<()> {
        match token {
            Some(token) => self.reply_message(token, &reply).await,
            None => Err(no_reply_stream("management query")),
        }
    }

    async fn sync_blocks(
        &self,
        root: Cid,
        providers: Vec<PeerId>,
        missing: Vec<Cid>,
    ) -> Result<QueryId> {
        self.sync(car_request(root, missing, None, Vec::new()), providers)
            .await
    }

    async fn sync_blocks_within(
        &self,
        root: Cid,
        providers: Vec<PeerId>,
        missing: Vec<Cid>,
        max_depth: u32,
        haves: Vec<Cid>,
    ) -> Result<QueryId> {
        self.sync(car_request(root, missing, Some(max_depth), haves), providers)
            .await
    }

    fn supports_cancellable_rooted_sync(&self) -> bool {
        true
    }

    async fn cancel_sync(&self, query_id: QueryId) -> Result<bool> {
        self.send_command(|reply| Command::CancelSync { query_id, reply })
            .await
    }

    async fn create_replicator(&self, peer_id: &PeerId, collections: Vec<String>) -> Result<()> {
        let info = ReplicatorInfo::from_raw(peer_id.to_string(), collections, Vec::new());
        self.create_replicator_info(peer_id, info).await
    }

    async fn create_replicator_info(
        &self,
        peer_id: &PeerId,
        mut info: ReplicatorInfo,
    ) -> Result<()> {
        info.id = peer_id.to_string();
        self.replicators.set_replicator_info(info);
        Ok(())
    }

    async fn delete_replicator(&self, peer_id: &PeerId) -> Result<()> {
        self.replicators.remove_peer(peer_id.as_str());
        self.explicit_replay_capabilities
            .clear_all(peer_id.as_str());
        Ok(())
    }

    async fn list_replicators(&self) -> Result<Vec<ReplicatorInfo>> {
        Ok(self.replicators.list_replicator_info())
    }

    async fn get_replicator(&self, peer_id: &PeerId) -> Result<Option<ReplicatorInfo>> {
        Ok(self.replicators.get_replicator_info(peer_id.as_str()))
    }

    async fn remove_replicator_collections(
        &self,
        peer_id: &PeerId,
        collections: Vec<String>,
    ) -> Result<bool> {
        let fully_deleted = self
            .replicators
            .remove_peer_collections(peer_id.as_str(), &collections);
        if fully_deleted {
            self.explicit_replay_capabilities
                .clear_all(peer_id.as_str());
        } else {
            self.explicit_replay_capabilities
                .clear(peer_id.as_str(), &collections);
        }
        Ok(fully_deleted)
    }

    async fn shutdown(&self) -> Result<()> {
        self.send_command(|reply| Command::Shutdown { reply }).await
    }
}

impl VincTransport {
    async fn sync(&self, request: CarFetchRequest, providers: Vec<PeerId>) -> Result<QueryId> {
        self.send_command(|reply| Command::SyncBlocks {
            request,
            providers,
            reply,
        })
        .await
    }
}

/// The CAR request a block sync sends each provider: the whole DAG below
/// `root`, or the closure below the missing frontier, cut at `max_depth`.
fn car_request(
    root: Cid,
    missing: Vec<Cid>,
    max_depth: Option<u32>,
    haves: Vec<Cid>,
) -> CarFetchRequest {
    if missing.is_empty() {
        CarFetchRequest::full_dag(root)
    } else {
        CarFetchRequest {
            max_depth,
            ..CarFetchRequest::selective_dag(root, missing)
        }
        .with_haves(haves)
    }
}

/// Every reply goes on its request's stream, which every request carries; a
/// reply without one has nowhere to go.
fn no_reply_stream(kind: &str) -> Error {
    Error::Transport(format!(
        "a vinc {kind} reply goes on its request's stream, and this one has none"
    ))
}

/// Resolves a peer's DID by the identity challenge, over the connection vinc
/// authenticated. No cache: each serve-boundary check asks again.
#[derive(Clone)]
pub struct VincPeerIdentityResolver {
    transport: VincTransport,
}

impl VincPeerIdentityResolver {
    pub fn new(transport: VincTransport) -> Self {
        Self { transport }
    }
}

/// How long a serve-boundary identity check waits for the peer.
const IDENTITY_DEADLINE: Duration = Duration::from_secs(10);

#[async_trait]
impl crate::peer_identity::PeerIdentityResolver for VincPeerIdentityResolver {
    async fn resolve(&self, peer_id: &PeerId) -> Option<identity::Did> {
        crate::rt::time::timeout(IDENTITY_DEADLINE, self.transport.get_peer_identity(peer_id))
            .await
            .ok()?
            .inspect_err(
                |error| tracing::debug!(%peer_id, %error, "vinc peer identity challenge failed"),
            )
            .ok()
            .flatten()
    }
}
