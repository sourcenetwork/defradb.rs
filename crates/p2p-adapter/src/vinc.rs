use std::sync::Arc;

use async_trait::async_trait;
use blockstore::Blockstore;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;

use crate::transport_doc_pusher::TransportDocPusher;
use crate::transport_version_syncer::TransportVersionSyncer;
use crate::{
    ExplicitReplayCapabilityInput, P2PError, P2PErrorExt as _, P2POperations, P2PResult,
    P2pDocumentInfo, P2pDocumentRequest, ReplicationFilters, ReplicatorInfo, ReplicatorPushOptions,
    ReplicatorPushOptionsState, TransportPeerId,
};

use p2p::sync::VincSyncCoordinator;
use p2p::topics::DefraTopic;
use p2p::vinc::{
    best_shareable_public_addr, canonical_peer_id, format_public_listen_addrs,
    parse_canonical_peer_id, parse_public_peer_addr, VincTransport,
};
use p2p::P2PTransport;

const DOC_SYNC_DISPATCH_PARALLELISM: usize = 16;

/// Wait for `collection_id`'s merges until none came for `idle_timeout`, or
/// `overall_timeout` from `start` passed. A copy of libp2p's wait.
async fn wait_for_branchable_merges(
    sub: &mut events::Subscription,
    collection_id: &str,
    start: web_time::Instant,
    overall_timeout: std::time::Duration,
    idle_timeout: std::time::Duration,
) {
    let mut saw_merge = false;
    let mut last_activity = web_time::Instant::now();
    while start.elapsed() < overall_timeout {
        if last_activity.elapsed() > idle_timeout {
            break;
        }
        match p2p::rt::time::timeout(std::time::Duration::from_millis(100), sub.recv()).await {
            Ok(Some(msg)) => {
                if msg
                    .as_merge_complete()
                    .is_some_and(|data| data.collection_id == collection_id)
                {
                    saw_merge = true;
                    last_activity = web_time::Instant::now();
                }
            }
            Ok(None) => break,
            Err(_) => {
                if !saw_merge && start.elapsed() > idle_timeout {
                    break;
                }
            }
        }
    }
}

/// P2P operations implementation for the vinc transport.
pub struct VincP2PAdapter<B: Blockstore + 'static> {
    transport: VincTransport,
    sync_coordinator: Option<Arc<VincSyncCoordinator<B>>>,
    doc_pusher: Option<Arc<dyn TransportDocPusher>>,
    event_bus: Option<Arc<dyn events::Bus>>,
    version_syncer: Option<Arc<dyn TransportVersionSyncer>>,
    replicator_push_options: ReplicatorPushOptionsState,
    peer_addresses: Arc<HopscotchMap<String, String, RandomState>>,
    tracked_documents: Arc<HopscotchMap<String, (), RandomState>>,
    nac_checker: Option<Arc<dyn db::NodeAccessChecker>>,
    network: p2p::network::Network,
}

impl<B: Blockstore + 'static> VincP2PAdapter<B> {
    async fn check_nac(&self, permission: acp::nac::NodePermission) -> P2PResult<()> {
        if let Some(ref checker) = self.nac_checker {
            checker
                .check_node_access(permission)
                .await
                .map_err(crate::map_nac_error)?;
        }
        Ok(())
    }

    /// Whether the caller holds `permission`, as a plain answer rather than an
    /// error.
    ///
    /// Used to resolve an authority that is carried into a state change, not to
    /// decide whether a call is allowed: a missing permission is a fact about
    /// the caller here, not a failure. A node with no access control configured
    /// holds everything, exactly as `check_nac` treats it.
    async fn holds_nac(&self, permission: acp::nac::NodePermission) -> bool {
        match self.nac_checker {
            Some(ref checker) => checker.check_node_access(permission).await.is_ok(),
            None => true,
        }
    }

    /// True when the transport already holds a live connection to `peer_id`.
    ///
    /// Comparison is in canonical id form (`canonical_peer_id`), so a base32
    /// spelling of the peer id still matches the transport's canonical-hex
    /// entries. A failed read degrades to `false`, so callers fall back to
    /// dialing exactly as they did before this check existed.
    async fn is_transport_connected(&self, peer_id: &p2p::transport::PeerId) -> bool {
        let canonical_id = canonical_peer_id(peer_id);
        self.transport
            .connected_peers()
            .await
            .map(|peers| {
                peers
                    .iter()
                    .any(|peer| canonical_peer_id(peer) == canonical_id)
            })
            .unwrap_or(false)
    }

    async fn resubscribe_tracked_document_topics(&self) {
        let doc_ids: Vec<String> = self.tracked_documents.keys().collect();
        for doc_id in &doc_ids {
            let topic = DefraTopic::document(doc_id);
            if let Err(error) = self.transport.unsubscribe(topic.clone()).await {
                tracing::debug!(doc_id = %doc_id, error = %error, "failed to drop tracked document topic before reconnect resubscribe");
            }
            if let Err(error) = self.transport.subscribe(topic).await {
                tracing::debug!(doc_id = %doc_id, error = %error, "failed to resubscribe tracked document topic after reconnect");
            }
        }
    }

    pub fn with_full_context(
        transport: VincTransport,
        coordinator: Arc<VincSyncCoordinator<B>>,
        doc_pusher: Arc<dyn TransportDocPusher>,
        event_bus: Arc<dyn events::Bus>,
        version_syncer: Option<Arc<dyn TransportVersionSyncer>>,
        nac_checker: Arc<dyn db::NodeAccessChecker>,
    ) -> Self {
        let network = coordinator.network();
        let tracked_documents = coordinator.followed_documents();
        Self {
            transport,
            sync_coordinator: Some(coordinator),
            doc_pusher: Some(doc_pusher),
            event_bus: Some(event_bus),
            version_syncer,
            replicator_push_options: ReplicatorPushOptionsState::default(),
            peer_addresses: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            tracked_documents,
            nac_checker: Some(nac_checker),
            network,
        }
    }

    pub fn with_replicator_push_options(mut self, options: ReplicatorPushOptions) -> Self {
        self.replicator_push_options = ReplicatorPushOptionsState::new(options);
        self
    }

    fn resolve_replication_filters(
        filters: ReplicationFilters,
        effective_collections: &[String],
        collection_cids: &[String],
    ) -> P2PResult<p2p::ReplicationFilters> {
        let mut resolved = p2p::ReplicationFilters::new();
        for (key, filter) in filters {
            let p2p_filter = if let Some(conds) = filter.conditions {
                p2p::ReplicationFilter::predicate(conds)
            } else {
                if filter.field.trim().is_empty() {
                    return Err(P2PError::invalid_input(
                        "replication filter field cannot be empty",
                    ));
                }
                if filter.value.is_null() || filter.value.is_array() || filter.value.is_object() {
                    return Err(P2PError::invalid_input(format!(
                        "replication filter for collection '{key}' must use a scalar value"
                    )));
                }
                p2p::ReplicationFilter::new(filter.field, filter.value)
            };

            let collection_id = collection_cids
                .iter()
                .position(|collection_id| collection_id == &key)
                .or_else(|| {
                    effective_collections
                        .iter()
                        .position(|collection_name| collection_name == &key)
                })
                .and_then(|index| collection_cids.get(index))
                .cloned()
                .ok_or_else(|| {
                    P2PError::invalid_input(format!(
                        "replication filter collection '{key}' was not requested"
                    ))
                })?;

            resolved.insert(collection_id, p2p_filter);
        }
        Ok(resolved)
    }

    pub fn with_replicator_push_options_state(
        mut self,
        options: ReplicatorPushOptionsState,
    ) -> Self {
        self.replicator_push_options = options;
        self
    }

    pub fn with_full_context_arc(
        transport: VincTransport,
        coordinator: Arc<VincSyncCoordinator<B>>,
        doc_pusher: Arc<dyn TransportDocPusher>,
        event_bus: Arc<dyn events::Bus>,
        version_syncer: Option<Arc<dyn TransportVersionSyncer>>,
        nac_checker: Arc<dyn db::NodeAccessChecker>,
    ) -> Arc<dyn P2POperations> {
        Arc::new(Self::with_full_context(
            transport,
            coordinator,
            doc_pusher,
            event_bus,
            version_syncer,
            nac_checker,
        ))
    }

    pub fn set_initial_tracked_documents(&self, docs: RapidHashSet<String>) {
        self.tracked_documents
            .extend(docs.into_iter().map(|doc_id| (doc_id, ())));
    }

    /// Resolve collection names to CIDs for removal, mirroring `add_replicator`.
    fn resolve_collections_for_remove(&self, collections: Vec<String>) -> Vec<String> {
        match self.doc_pusher {
            Some(ref pusher) => crate::resolve_remove_collections(collections, |name| {
                pusher.get_collection_id(name)
            }),
            None => collections,
        }
    }

    /// Drop every trace of a revoked peer's replication, so nothing dials it
    /// again on a timer. Mirrors the full-deletion branch of
    /// `remove_replicator` (all collections, not a subset), plus the durable
    /// retry record that survives a restart.
    ///
    /// Idempotent: a peer that was never a replicator has nothing to delete
    /// and that is not an error, which matters because revoking a peer that
    /// never replicated is a perfectly ordinary thing to do.
    async fn deregister_revoked_replicator(
        &self,
        peer_id: &p2p::transport::PeerId,
    ) -> P2PResult<()> {
        if let Some(ref coordinator) = self.sync_coordinator {
            coordinator
                .delete_replicator(peer_id)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
        } else {
            self.transport
                .delete_replicator(peer_id)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
        }

        if let Some(ref pusher) = self.doc_pusher {
            pusher
                .delete_persisted_replicator(&peer_id.to_string())
                .await?;
        }

        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Blockstore + 'static> P2POperations for VincP2PAdapter<B> {
    fn bind_request(&self, context: &defra_core::RequestContext) -> Option<Arc<dyn P2POperations>> {
        Some(Arc::new(Self {
            transport: self.transport.clone(),
            sync_coordinator: self.sync_coordinator.clone(),
            doc_pusher: self.doc_pusher.clone(),
            event_bus: self.event_bus.clone(),
            version_syncer: self.version_syncer.clone(),
            replicator_push_options: self.replicator_push_options.clone(),
            peer_addresses: Arc::clone(&self.peer_addresses),
            tracked_documents: Arc::clone(&self.tracked_documents),
            nac_checker: self
                .nac_checker
                .as_ref()
                .map(|checker| db::bound_node_access_checker(Arc::clone(checker), context.clone())),
            network: self.network.clone(),
        }))
    }

    async fn replication_acknowledged(
        &self,
        peer_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        self.check_nac(acp::nac::NodePermission::P2pReplicatorList)
            .await?;
        let Some(pusher) = self.doc_pusher.as_ref() else {
            return Err(P2PError::unsupported(
                "replication acknowledgement needs a document pusher",
            ));
        };
        pusher
            .replication_acknowledged(peer_id, collection_name, doc_ids)
            .await
    }

    async fn document_heads_acknowledged(
        &self,
        peer_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        self.check_nac(acp::nac::NodePermission::P2pReplicatorList)
            .await?;
        let Some(pusher) = self.doc_pusher.as_ref() else {
            return Err(P2PError::unsupported(
                "document head acknowledgement needs a document pusher",
            ));
        };
        pusher
            .document_heads_acknowledged(peer_id, collection_name, doc_ids)
            .await
    }

    async fn sync_status(&self) -> P2PResult<serde_json::Value> {
        let Some(coordinator) = self.sync_coordinator.as_ref() else {
            return Ok(serde_json::Value::Null);
        };
        let mut status = serde_json::to_value(coordinator.sync_status().await)
            .map_err(|error| P2PError::transport(error.to_string()))?;
        if let (Some(pusher), Some(object)) = (self.doc_pusher.as_ref(), status.as_object_mut()) {
            object.insert(
                "push_retry_markers".to_string(),
                serde_json::to_value(pusher.push_retry_marker_stats().await?)
                    .map_err(|error| P2PError::transport(error.to_string()))?,
            );
        }
        Ok(status)
    }

    async fn peer_details(&self, peer_id: &TransportPeerId) -> P2PResult<serde_json::Value> {
        let Some(coordinator) = self.sync_coordinator.as_ref() else {
            return Ok(serde_json::Value::Null);
        };
        let peer = parse_canonical_peer_id(peer_id.as_str())
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        let details = coordinator.peer_details(&peer).await;
        let identity = if details.history.connected || details.connection.is_some() {
            Some(self.resolve_peer_identity(peer_id).await)
        } else {
            None
        };
        let push_retries = match self.doc_pusher.as_ref() {
            Some(pusher) => pusher.push_retries_for_peer(peer_id.as_str()).await,
            None => Ok(None),
        };
        crate::peer_details::peer_details_json(details, identity, push_retries)
    }

    async fn local_peer_id(&self) -> P2PResult<String> {
        Ok(self.transport.local_peer_id().to_string())
    }

    async fn listen_addresses(&self) -> P2PResult<Vec<String>> {
        self.transport
            .listen_addresses()
            .await
            .map(|addrs| format_public_listen_addrs(self.transport.local_peer_id(), &addrs))
            .map_err(|error| P2PError::transport(error.to_string()))
    }

    async fn shareable_address(&self) -> P2PResult<Option<String>> {
        self.transport
            .listen_addresses()
            .await
            .map(|addrs| best_shareable_public_addr(self.transport.local_peer_id(), &addrs))
            .map_err(|error| P2PError::transport(error.to_string()))
    }

    async fn connected_peers(&self) -> P2PResult<Vec<String>> {
        self.check_nac(acp::nac::NodePermission::P2pPeerActive)
            .await?;

        let connected = self
            .transport
            .connected_peers()
            .await
            .map_err(|error| P2PError::transport(error.to_string()))?;

        let mut result = Vec::new();
        for peer in &connected {
            let peer_str = peer.to_string();
            if let Some(addr) = self.peer_addresses.get(&peer_str) {
                result.push(addr);
                continue;
            }
            // A peer that dialed in: vinc sends from the socket it listens on,
            // so the address it was seen at is the one to list.
            let seen_at = self
                .transport
                .peer_connection(peer)
                .await
                .ok()
                .flatten()
                .and_then(|connection| connection.remote_address);
            match seen_at {
                Some(remote) => result.push(format!("{peer_str}@{remote}")),
                None => result.push(peer_str),
            }
        }
        Ok(result)
    }

    async fn resolve_peer_identity(
        &self,
        peer_id: &TransportPeerId,
    ) -> P2PResult<Option<identity::Did>> {
        self.check_nac(acp::nac::NodePermission::P2pPeerActive)
            .await?;
        let peer_id = parse_canonical_peer_id(peer_id.as_str())
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        self.transport
            .get_peer_identity(&peer_id)
            .await
            .map_err(|error| P2PError::transport(error.to_string()))
    }

    async fn connect_peer(&self, addr: &str) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pPeerConnect)
            .await?;

        let (peer_id, direct_addrs) = parse_public_peer_addr(addr)
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        // Connecting to an already-connected peer is a no-op (matching Go
        // DefraDB, where libp2p `Connect` returns immediately when connected):
        // a redundant dial is not just wasted work — on Linux it can time out
        // against a healthy connection and fail the caller. Refresh the
        // address book and return.
        if self.is_transport_connected(&peer_id).await {
            self.peer_addresses
                .insert(peer_id.to_string(), addr.to_string());
            return Ok(());
        }

        let dial_timeout = if direct_addrs.is_empty() {
            std::time::Duration::from_secs(10)
        } else {
            std::time::Duration::from_secs(5)
        };

        p2p::rt::time::timeout(dial_timeout, self.transport.dial(&peer_id, direct_addrs))
            .await
            .map_err(|_| {
                P2PError::transport(format!("failed to connect: timeout dialing {peer_id}"))
            })?
            .map_err(|error| match error {
                p2p::error::Error::Unauthorized(message) => P2PError::unauthorized(message),
                error => P2PError::transport(format!("failed to connect: {error}")),
            })?;

        self.peer_addresses
            .insert(peer_id.to_string(), addr.to_string());
        self.resubscribe_tracked_document_topics().await;

        Ok(())
    }

    async fn disconnect_peer(&self, addr: &str) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pPeerDisconnect)
            .await?;

        let (peer_id, _direct_addrs) = parse_public_peer_addr(addr)
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        self.transport
            .disconnect(&peer_id)
            .await
            .map_err(|error| P2PError::transport(error.to_string()))?;
        self.peer_addresses.remove(&peer_id.to_string());
        Ok(())
    }

    /// Authorize a peer, which for a peer that is currently REVOKED means
    /// reversing a revocation.
    ///
    /// Ordinary admission needs `P2pPeerConnect`, as before. Lifting a
    /// revocation additionally needs `P2pPeerDisconnect`, the permission that
    /// could have imposed it: otherwise a principal provisioned only to add
    /// peers could undo a revocation it was never trusted to make, and the
    /// revocation would only be as strong as the weakest permission anyone
    /// holds.
    ///
    /// The second permission is resolved into an authority and carried down,
    /// not checked here. Deciding it here and acting on it in the endpoint
    /// would be a check against state that a concurrent revoke may already
    /// have changed; the endpoint applies the authority inside the same lock
    /// as the transition instead.
    async fn allow_peer(&self, peer_id: &TransportPeerId) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pPeerConnect)
            .await?;
        let may_revoke = self
            .holds_nac(acp::nac::NodePermission::P2pPeerDisconnect)
            .await;

        let peer_id = parse_canonical_peer_id(peer_id.as_str())
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        self.transport
            .allow_peer(
                &peer_id,
                p2p::vinc::AdmissionAuthority::new(true, may_revoke),
            )
            .await
            .map_err(|error| P2PError::transport(error.to_string()))
    }

    /// Bar a peer and stop this node reaching for it again.
    ///
    /// Two steps, and both are needed. `transport.deny_peer` bars the peer in
    /// the endpoint and hangs up what it holds, but a peer registered as a
    /// replicator is also something this node dials on a timer: the reconnect
    /// sweep dials exactly the registered peers missing from
    /// `connected_peers`, and hanging up is what makes it missing. Leaving the
    /// registration in place would mean a barred peer the node keeps trying to
    /// reach every couple of seconds, forever.
    ///
    /// Deregistration is deliberately destructive and is not undone by
    /// `allow_peer`: re-admitting a device restores its ability to connect,
    /// not its replication. The replicator has to be added back explicitly.
    ///
    /// The bar is applied FIRST so that a failure to deregister still leaves
    /// the peer barred rather than half-revoked, and the failure is returned
    /// rather than swallowed.
    async fn deny_peer(&self, peer_id: &TransportPeerId) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pPeerDisconnect)
            .await?;

        let peer_id = parse_canonical_peer_id(peer_id.as_str())
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        self.transport
            .deny_peer(&peer_id)
            .await
            .map_err(|error| P2PError::transport(error.to_string()))?;

        self.deregister_revoked_replicator(&peer_id).await
    }

    async fn notify_network_change(&self) -> P2PResult<()> {
        self.transport
            .network_change()
            .await
            .map_err(|error| P2PError::transport(error.to_string()))
    }

    async fn get_replicators(&self) -> P2PResult<Vec<ReplicatorInfo>> {
        self.check_nac(acp::nac::NodePermission::P2pReplicatorList)
            .await?;

        // #1074: report the LIVE replicator registry, not the persisted peerstore,
        // so a reconciler can observe authorization drift. The coordinator and the
        // raw transport read the same shared registry (the coordinator delegates to
        // the same transport), so both branches are equally live-authoritative;
        // persisted rows only overlay address/status metadata below.
        let p2p_infos = if let Some(ref coordinator) = self.sync_coordinator {
            coordinator
                .list_replicators()
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?
        } else {
            self.transport
                .list_replicators()
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?
        };
        let p2p_infos = if let Some(ref pusher) = self.doc_pusher {
            crate::merge_live_replicators_with_persisted_metadata(
                p2p_infos,
                pusher.load_persisted_replicators().await?,
            )
        } else {
            p2p_infos
        };

        Ok(p2p_infos
            .into_iter()
            .map(crate::to_http_replicator_info)
            .collect())
    }

    async fn add_replicator(
        &self,
        collections: Vec<String>,
        addr: Option<&str>,
        filters: ReplicationFilters,
        explicit_replay_capabilities: Vec<ExplicitReplayCapabilityInput>,
        expected_authorizer_did: Option<&str>,
    ) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pReplicatorAdd)
            .await?;

        let addr_str = addr.ok_or_else(|| P2PError::invalid_input("address is required"))?;
        let (peer_id, direct_addrs) = parse_public_peer_addr(addr_str)
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;

        let effective_collections = if collections.is_empty() {
            if let Some(ref pusher) = self.doc_pusher {
                pusher.list_collections()?
            } else {
                return Err(P2PError::unsupported(
                    "no database context to list collections",
                ));
            }
        } else {
            collections
        };

        let mut collection_cids = Vec::new();
        if let Some(ref pusher) = self.doc_pusher {
            for name in &effective_collections {
                if let Some(cid) = pusher.get_collection_id(name) {
                    collection_cids.push(cid);
                } else {
                    return Err(P2PError::not_found(format!(
                        "collection not found. Name: {name}"
                    )));
                }
            }
        } else {
            collection_cids.clone_from(&effective_collections);
        }
        let replication_filters =
            Self::resolve_replication_filters(filters, &effective_collections, &collection_cids)?;
        if !replication_filters.is_empty() {
            self.doc_pusher
                .as_ref()
                .ok_or_else(|| P2PError::unsupported("no database context to validate filters"))?
                .validate_replication_filters(&replication_filters)?;
        }

        let requested_collections: RapidHashSet<String> = collection_cids.iter().cloned().collect();
        let validated_capabilities = crate::validate_explicit_replay_capabilities(
            explicit_replay_capabilities,
            expected_authorizer_did,
            &requested_collections,
            self.transport.local_peer_id().as_str(),
            peer_id.as_str(),
        )?;
        // Same rationale as `connect_peer`: in the common pairing flow the
        // replicator is installed over an already-live connection, and a
        // redundant dial can spuriously time out (Linux). The registration and
        // initial replay below ride the existing connection; the address book
        // entry is still refreshed either way.
        if !self.is_transport_connected(&peer_id).await {
            self.transport
                .dial(&peer_id, direct_addrs)
                .await
                .map_err(|error| {
                    P2PError::transport(format!("failed to connect to replicator peer: {error}"))
                })?;
        }

        self.peer_addresses
            .insert(peer_id.to_string(), addr_str.to_string());

        let replicator_info = p2p::ReplicatorInfo::from_raw_with_filters(
            peer_id.to_string(),
            collection_cids.clone(),
            vec![addr_str.to_string()],
            replication_filters.clone(),
        );
        let mut replay = if let Some(pusher) = &self.doc_pusher {
            let request = crate::ReplicatorInstallRequest::new(
                replicator_info.clone(),
                effective_collections.clone(),
                &validated_capabilities,
            );
            let plan = pusher.prepare_replicator_install(&request).await?;
            Some(crate::replicator_replay::ReplayOwner::new(
                Arc::clone(pusher),
                plan,
                self.event_bus.clone(),
                self.network.clone(),
            ))
        } else {
            None
        };
        self.transport
            .clear_explicit_replay_capabilities(&peer_id, &collection_cids);
        for (collection_id, capability) in validated_capabilities {
            self.transport
                .set_explicit_replay_capability(&peer_id, &collection_id, &capability)
                .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        }

        if let Some(ref coordinator) = self.sync_coordinator {
            coordinator
                .create_replicator_info(&peer_id, replicator_info.clone(), false)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
        } else {
            self.transport
                .create_replicator_info(&peer_id, replicator_info)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
        }

        // Re-check the bar now the records exist, and undo them if the peer was
        // revoked while this was running.
        //
        // Registering a replicator is several awaits long (validation, a dial,
        // a durable write, then the live record), and `deny_peer` is not one
        // step either: it bars the peer and then deletes its replicator state.
        // Checking admission only at the start would let a registration that
        // began before the bar finish after the deletion and put both records
        // back, leaving a revoked peer registered and the reconnect sweep
        // dialling it forever.
        //
        // Ordering makes the pair safe without a lock, the same way the accept
        // and dial paths are made safe. `deny_peer` sets the bar BEFORE it
        // deletes, so either its deletion runs after these creations and
        // removes them, or the bar was already set when this check reads it and
        // this removes them. There is no interleaving in which the records
        // survive.
        // A failed check counts as revoked, not as permission. This is the
        // last gate before a peer keeps durable replication state, so the
        // safe answer to "we could not find out" is to undo the registration
        // and make the caller retry, not to assume the peer is fine.
        let revoked = match self.transport.is_peer_revoked(&peer_id).await {
            Ok(revoked) => revoked,
            Err(error) => {
                tracing::warn!(
                    peer_id = %peer_id,
                    error = %error,
                    "could not confirm peer admission after registering its replicator; \
                     rolling the registration back"
                );
                true
            }
        };
        if revoked {
            self.deregister_revoked_replicator(&peer_id).await?;
            return Err(P2PError::invalid_input(format!(
                "peer {peer_id} is not admitted, or its admission could not be confirmed, \
                 while its replicator was being registered; the registration has been \
                 rolled back and can be retried"
            )));
        }

        if let Some(mut replay) = replay.take() {
            let Some(plan) = replay.plan.as_ref() else {
                return Err(P2PError::internal("replicator replay has no install plan"));
            };
            let collections = plan.replay.clone();
            let install_incarnation = plan.id;
            if !collections.is_empty() {
                let push_pusher =
                    Arc::clone(self.doc_pusher.as_ref().expect("replay has a pusher"));
                let push_peer = peer_id;
                let push_options = self.replicator_push_options.load();
                let push_se_key = push_options.se_encryption_key;
                let push_identity = push_options.se_identity_pubkey;
                let push_filters = replication_filters.clone();

                tracing::info!(
                    peer_id = %push_peer,
                    replay_collections = ?collections,
                    "Replaying existing docs for collections requiring replay"
                );

                self.network.spawn(async move {
                    let result = push_pusher
                        .push_existing_docs(
                            &push_peer,
                            &collections,
                            &push_filters,
                            push_se_key.as_ref().map(|key| key.as_slice()),
                            push_identity.as_deref(),
                            install_incarnation,
                        )
                        .await;
                    if let Err(ref error) = result {
                        tracing::error!(error = %error, "Failed to push existing docs to replicator");
                    }
                    replay
                        .complete(result.err().map(|error| error.to_string()))
                        .await;
                });
            } else {
                replay.complete(None).await;
            }
        } else {
            tracing::debug!(
                peer_id = %peer_id,
                "Replicator already exists with same collections, filters, and replay capability; skipping initial replay"
            );
            if let Some(ref bus) = self.event_bus {
                bus.publish(events::Message::replicator_completed_with_data(
                    events::ReplicatorCompletedData {
                        peer_id: peer_id.to_string(),
                        collections: effective_collections.to_vec(),
                        skipped: true,
                        error: None,
                    },
                ));
            }
        }

        Ok(())
    }

    async fn remove_replicator(
        &self,
        collections: Vec<String>,
        addr: Option<&str>,
    ) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pReplicatorDelete)
            .await?;

        let addr_str = addr.ok_or_else(|| P2PError::invalid_input("address is required"))?;
        let (peer_id, _direct_addrs) = parse_public_peer_addr(addr_str)
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;

        // Push registry is CID-keyed; resolve names symmetric with add_replicator.
        let collections = self.resolve_collections_for_remove(collections);

        let fully_deleted = if let Some(ref coordinator) = self.sync_coordinator {
            coordinator
                .remove_replicator_collections(&peer_id, collections)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?
        } else if collections.is_empty() {
            self.transport
                .delete_replicator(&peer_id)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
            true
        } else {
            self.transport
                .remove_replicator_collections(&peer_id, collections)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?
        };

        if let Some(ref pusher) = self.doc_pusher {
            if fully_deleted {
                pusher
                    .delete_persisted_replicator(&peer_id.to_string())
                    .await?;
            } else {
                let remaining = self
                    .transport
                    .get_replicator(&peer_id)
                    .await
                    .map_err(|error| P2PError::transport(error.to_string()))?;
                if let Some(info) = remaining {
                    if let Err(error) = pusher
                        .persist_replicator(&peer_id.to_string(), &info.collections)
                        .await
                    {
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = %error,
                            "Failed to update persisted replicator"
                        );
                    }
                }
            }
        }

        if let Some(ref bus) = self.event_bus {
            bus.publish(events::Message::replicator_completed());
        }

        Ok(())
    }

    async fn get_collections(&self) -> P2PResult<Vec<String>> {
        self.check_nac(acp::nac::NodePermission::P2pCollectionList)
            .await?;

        if let Some(ref coordinator) = self.sync_coordinator {
            let collection_ids = coordinator
                .get_subscribed_collections()
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;
            if let Some(ref pusher) = self.doc_pusher {
                collection_ids
                    .into_iter()
                    .map(|collection_id| {
                        pusher.get_collection_name(&collection_id)?.ok_or_else(|| {
                            P2PError::not_found(format!(
                                "collection with ID '{collection_id}' not found"
                            ))
                        })
                    })
                    .collect()
            } else {
                Ok(collection_ids)
            }
        } else {
            Ok(Vec::new())
        }
    }

    async fn add_collections(&self, collections: Vec<String>) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pCollectionAdd)
            .await?;

        if let Some(ref coordinator) = self.sync_coordinator {
            let topic_ids = collections
                .into_iter()
                .map(|collection_name| {
                    if let Some(ref pusher) = self.doc_pusher {
                        pusher.get_collection_id(&collection_name).ok_or_else(|| {
                            P2PError::not_found(format!(
                                "collection not found. Name: {}",
                                collection_name
                            ))
                        })
                    } else {
                        Ok(collection_name)
                    }
                })
                .collect::<P2PResult<Vec<_>>>()?;

            coordinator
                .subscribe_collections(&topic_ids)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;

            Ok(())
        } else {
            Err(P2PError::unsupported(
                "p2p collections functionality requires sync coordinator",
            ))
        }
    }

    async fn remove_collections(&self, collections: Vec<String>) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pCollectionDelete)
            .await?;

        if let Some(ref coordinator) = self.sync_coordinator {
            let topic_ids = collections
                .into_iter()
                .map(|collection_name| {
                    if let Some(ref pusher) = self.doc_pusher {
                        pusher.get_collection_id(&collection_name).ok_or_else(|| {
                            P2PError::not_found(format!(
                                "collection not found. Name: {}",
                                collection_name
                            ))
                        })
                    } else {
                        Ok(collection_name)
                    }
                })
                .collect::<P2PResult<Vec<_>>>()?;

            coordinator
                .unsubscribe_collections(&topic_ids)
                .await
                .map_err(|error| P2PError::transport(error.to_string()))?;

            Ok(())
        } else {
            Err(P2PError::unsupported(
                "p2p collections functionality requires sync coordinator",
            ))
        }
    }

    async fn get_documents(&self) -> P2PResult<Vec<P2pDocumentInfo>> {
        self.check_nac(acp::nac::NodePermission::P2pDocumentList)
            .await?;

        let mut sorted: Vec<String> = self.tracked_documents.keys().collect();
        sorted.sort();
        Ok(sorted
            .into_iter()
            .map(|doc_id| P2pDocumentInfo {
                collection: String::new(),
                doc_id,
            })
            .collect())
    }

    async fn add_documents(&self, docs: Vec<P2pDocumentRequest>) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pDocumentAdd)
            .await?;

        let doc_ids: Vec<String> = docs.into_iter().map(|doc| doc.doc_id).collect();
        document::validate_doc_ids(&doc_ids).map_err(|_| {
            P2PError::invalid_input("malformed document ID, missing either version or cid")
        })?;

        for doc_id in &doc_ids {
            self.tracked_documents.insert(doc_id.clone(), ());
            let topic = DefraTopic::document(doc_id);
            if let Err(error) = self.transport.subscribe(topic).await {
                tracing::warn!(doc_id = %doc_id, error = %error, "Failed to subscribe to topic for document");
            }
        }

        if let Some(ref pusher) = self.doc_pusher {
            let all_docs: Vec<String> = self.tracked_documents.keys().collect();
            if let Err(error) = pusher.persist_p2p_documents(&all_docs).await {
                tracing::warn!(error = %error, "failed to persist P2P documents");
            }
        }

        Ok(())
    }

    async fn remove_documents(&self, docs: Vec<P2pDocumentRequest>) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pDocumentDelete)
            .await?;

        let doc_ids: Vec<String> = docs.into_iter().map(|doc| doc.doc_id).collect();
        document::validate_doc_ids(&doc_ids).map_err(|_| {
            P2PError::invalid_input("malformed document ID, missing either version or cid")
        })?;

        for doc_id in &doc_ids {
            self.tracked_documents.remove(doc_id);
            let topic = DefraTopic::document(doc_id);
            if let Err(error) = self.transport.unsubscribe(topic).await {
                tracing::warn!(doc_id = %doc_id, error = %error, "Failed to unsubscribe from topic for document");
            }
        }

        if let Some(ref pusher) = self.doc_pusher {
            let all_docs: Vec<String> = self.tracked_documents.keys().collect();
            if let Err(error) = pusher.persist_p2p_documents(&all_docs).await {
                tracing::warn!(error = %error, "failed to persist P2P documents after removal");
            }
        }

        Ok(())
    }

    async fn sync_documents(
        &self,
        collection_name: &str,
        doc_ids: Vec<String>,
        timeout: Option<std::time::Duration>,
    ) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pSyncDocuments)
            .await?;
        let pusher = self
            .doc_pusher
            .as_ref()
            .ok_or_else(|| P2PError::unsupported("no database context for sync"))?;
        pusher.validate_collection_exists(collection_name)?;

        let event_bus = self
            .event_bus
            .as_ref()
            .ok_or_else(|| P2PError::unsupported("no event bus for sync"))?;

        crate::doc_sync::sync::sync_documents(
            Arc::new(self.transport.clone()),
            event_bus.as_ref(),
            &self.network,
            doc_ids,
            timeout.unwrap_or(crate::doc_sync::DEFAULT_DOC_SYNC_TIMEOUT),
            DOC_SYNC_DISPATCH_PARALLELISM,
        )
        .await
    }

    async fn sync_branchable_collection(&self, collection_id: &str) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pSyncBranchableCollection)
            .await?;
        let pusher = self
            .doc_pusher
            .as_ref()
            .ok_or_else(|| P2PError::unsupported("no database context for sync"))?;
        pusher.validate_branchable_collection(collection_id)?;

        let connected_peers = self.transport.connected_peers().await.map_err(|error| {
            P2PError::transport(format!("failed to get connected peers: {error}"))
        })?;
        if connected_peers.is_empty() {
            return Ok(());
        }

        let mut request = p2p::message::BranchableSyncRequest::new(collection_id.to_string());
        p2p::signing::sign_with_transport(&self.transport, &mut request).map_err(|error| {
            P2PError::internal(format!("failed to sign BranchableSync request: {error}"))
        })?;

        // As on libp2p, the call returns once the merges the replies started
        // have gone quiet, so a read right after it sees the synced documents.
        let mut merges = self.event_bus.as_ref().map(|bus| {
            (
                Arc::clone(bus),
                bus.subscribe(&[events::EventName::MergeComplete]),
            )
        });
        let start = web_time::Instant::now();
        for peer in &connected_peers {
            if let Err(error) = self
                .transport
                .send_branchable_sync_request(peer, request.clone())
                .await
            {
                tracing::warn!(peer_id = %peer, error = %error, "failed to send BranchableSyncRequest");
            }
        }
        if let Some((bus, sub)) = merges.as_mut() {
            wait_for_branchable_merges(
                sub,
                collection_id,
                start,
                std::time::Duration::from_secs(30),
                std::time::Duration::from_secs(3),
            )
            .await;
            bus.unsubscribe(sub.id());
        }
        Ok(())
    }

    async fn sync_collection_versions(&self, version_ids: Vec<String>) -> P2PResult<()> {
        self.check_nac(acp::nac::NodePermission::P2pSyncCollectionVersions)
            .await?;
        if version_ids.is_empty() {
            return Ok(());
        }
        for version_id in &version_ids {
            cid::Cid::try_from(version_id.as_str())
                .map_err(|error| P2PError::invalid_input(format!("invalid cid: {error}")))?;
        }

        let connected_peers = self.transport.connected_peers().await.map_err(|error| {
            P2PError::transport(format!("failed to get connected peers: {error}"))
        })?;
        if connected_peers.is_empty() {
            return Ok(());
        }

        let syncer = self
            .version_syncer
            .as_ref()
            .ok_or_else(|| P2PError::unsupported("version syncer required"))?;
        // The call returns once the versions are synced, as libp2p's and Go's do.
        syncer.sync_versions(version_ids, connected_peers).await
    }

    async fn push_documents_to_peer(
        &self,
        peer_id: &str,
        docs: Vec<P2pDocumentRequest>,
    ) -> P2PResult<()> {
        let (peer_id, _direct_addrs) = parse_public_peer_addr(peer_id)
            .map_err(|error| P2PError::invalid_input(error.to_string()))?;
        let pusher = self
            .doc_pusher
            .as_ref()
            .ok_or_else(|| P2PError::unsupported("no database context for document push"))?;
        let pairs: Vec<(String, String)> = docs
            .into_iter()
            .map(|doc| (doc.collection, doc.doc_id))
            .collect();
        pusher
            .push_existing_docs_by_id(&peer_id, &pairs, &db::merge::SelectedPushStop::new())
            .await
    }
}
