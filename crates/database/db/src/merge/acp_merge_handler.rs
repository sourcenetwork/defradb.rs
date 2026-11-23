use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use cid::Cid;
use defra_core::merge::{
    BlockMetadata, ExplicitReplayAuthorization, MergeBlock, MergeHandler, MergeOutcome,
    RecoveredBlockMetadata,
};
use identity::Identity as _;
use schema::CollectionVersion;
use storage::corekv::Store;

use crate::merge::merge_handler::hook::{
    CompositeFrame, CompositeMergeHook, CompositePostCommitAction,
};
use crate::merge::merge_handler::{DbMergeHandler, MergeError};
use crate::merge::replica::{ReplicaRegistration, ReplicaStep};

pub type AcpMergeError = MergeError;

struct RegisterReplicatedDocAction {
    acp: Arc<dyn DocumentACP>,
    did: identity::Did,
    policy_id: String,
    resource_name: String,
    doc_id: String,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CompositePostCommitAction for RegisterReplicatedDocAction {
    async fn run(self: Box<Self>) -> Result<(), MergeError> {
        self.acp
            .register_doc_object(
                &self.did,
                &self.policy_id,
                &self.resource_name,
                &self.doc_id,
            )
            .await
            .map_err(|e| MergeError::MergeFailed(format!("ACP registration failed: {}", e)))
    }
}

/// After a merge into a document of a listed collection: submit what the
/// node owes on it as a replica, if anything.
struct SettleReplicaAction {
    acp: Arc<dyn DocumentACP>,
    policy_id: String,
    resource_name: String,
    doc_id: String,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CompositePostCommitAction for SettleReplicaAction {
    async fn run(self: Box<Self>) -> Result<(), MergeError> {
        self.acp
            .settle_replica(&self.policy_id, &self.resource_name, &self.doc_id)
            .await
            .map_err(|e| MergeError::MergeFailed(format!("replica settlement failed: {}", e)))
    }
}

pub struct AcpCompositeMergeHook {
    document_acp: regolith::sync::OnceCell<Arc<dyn DocumentACP>>,
    local_identity: Option<Identity>,
    strict_replicated_doc_access: AtomicBool,
    replicas: regolith::sync::OnceCell<Arc<ReplicaRegistration>>,
}

impl AcpCompositeMergeHook {
    pub fn new(local_identity: Option<Identity>) -> Self {
        Self {
            document_acp: regolith::sync::OnceCell::new(),
            local_identity,
            strict_replicated_doc_access: AtomicBool::new(false),
            replicas: regolith::sync::OnceCell::new(),
        }
    }

    /// Register replicated documents of the collections `replicas` lists
    /// with a remote provider ([`crate::merge::replica`]).
    pub fn set_replica_registration(&self, replicas: Arc<ReplicaRegistration>) {
        let _ = self.replicas.set(replicas);
    }

    fn strict(&self) -> bool {
        self.strict_replicated_doc_access.load(Ordering::Relaxed)
    }

    /// The replica registration that covers `collection` on a remote
    /// provider under strict access, if any.
    fn replicas_for(&self, collection: &CollectionVersion) -> Option<&Arc<ReplicaRegistration>> {
        let acp = self.document_acp()?;
        if !self.strict() || acp.supports_transactions() || collection.policy.is_none() {
            return None;
        }
        self.replicas
            .get()
            .filter(|replicas| replicas.lists(collection))
    }

    /// Whether this node can read the document: as its identity, or as its
    /// provider's own actor, which is the same node and the DID it registers
    /// replicated documents as.
    async fn node_reads(
        &self,
        acp: &dyn DocumentACP,
        identity: &Identity,
        policy: &schema::PolicyDescription,
        doc_id: &str,
    ) -> Result<bool, MergeError> {
        let reads = |who: Identity| async move {
            acp.check_doc_access(
                &who,
                DocumentPermission::Read,
                &policy.id,
                &policy.resource_name,
                doc_id,
            )
            .await
            .map_err(|e| MergeError::MergeFailed(format!("ACP access check failed: {}", e)))
        };
        if reads(identity.clone()).await? {
            return Ok(true);
        }
        match acp.own_actor().map(Identity::Authenticated) {
            Some(actor) if actor != *identity => reads(actor).await,
            _ => Ok(false),
        }
    }

    pub fn set_document_acp(&self, acp: Arc<dyn DocumentACP>) {
        let _ = self.document_acp.set(acp);
    }

    pub fn set_strict_replicated_doc_access(&self, strict: bool) {
        self.strict_replicated_doc_access
            .store(strict, Ordering::Relaxed);
    }

    fn document_acp(&self) -> Option<&Arc<dyn DocumentACP>> {
        self.document_acp.get()
    }

    async fn signer_may(
        &self,
        acp: &dyn DocumentACP,
        signer: identity::Did,
        permission: DocumentPermission,
        policy: &schema::PolicyDescription,
        doc_id: &str,
    ) -> Result<bool, MergeError> {
        let signer = Identity::Authenticated(signer);
        // Keep the local-ACP node-owner shortcut, but never use it for
        // Vera: a node's signature is not a shared-policy write grant.
        if !self.strict_replicated_doc_access.load(Ordering::Relaxed)
            && self.local_identity.as_ref() == Some(&signer)
        {
            return Ok(true);
        }
        acp.check_doc_access(
            &signer,
            permission,
            &policy.id,
            &policy.resource_name,
            doc_id,
        )
        .await
        .map_err(|e| MergeError::MergeFailed(format!("ACP access check failed: {}", e)))
    }

    /// Before a change to a replicated document is put back because this
    /// node still owes something on it, try once to pay it, so the next try
    /// can merge. A failure leaves it owed for a later merge, the next
    /// start, or a later try.
    async fn settle_owed_replica(
        &self,
        acp: &dyn DocumentACP,
        policy: &schema::PolicyDescription,
        doc_id: &str,
    ) {
        if let Err(error) = acp
            .settle_replica(&policy.id, &policy.resource_name, doc_id)
            .await
        {
            tracing::debug!(%doc_id, %error, "What is owed on a replicated document stays owed");
        }
    }
}

const DELETED_STATUS: u8 = 2;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CompositeMergeHook for AcpCompositeMergeHook {
    fn supports_transactions(&self) -> bool {
        self.document_acp()
            .is_some_and(|acp| acp.supports_transactions())
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    fn bind_transaction(
        &self,
        view: datastore::NamespaceView,
    ) -> Option<Arc<dyn CompositeMergeHook>> {
        let acp = self.document_acp()?.bind_transaction(view)?;
        let hook = Self::new(self.local_identity.clone());
        hook.set_document_acp(acp);
        hook.set_strict_replicated_doc_access(
            self.strict_replicated_doc_access.load(Ordering::Relaxed),
        );
        if let Some(replicas) = self.replicas.get() {
            hook.set_replica_registration(Arc::clone(replicas));
        }
        Some(Arc::new(hook))
    }

    fn guards_protected_updates(&self) -> bool {
        self.document_acp().is_some()
    }

    async fn on_protected_update(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
        frame: CompositeFrame<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        let Some(acp) = self.document_acp() else {
            return Ok(None);
        };
        let Some(policy) = &collection.policy else {
            return Ok(None);
        };
        // Receiver read access (including explicit replay) is independent of
        // the verified signer's authority to update this particular composite.
        if frame.is_genesis {
            return Ok(None);
        }
        let is_registered = acp
            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| {
                MergeError::MergeFailed(format!("ACP registration lookup failed: {}", e))
            })?;
        if !is_registered {
            // Local ACP treats unregistered replicas as public. Vera
            // registration can lag replication; never turn that lag into a
            // write-authorization bypass, even on an explicit replay path.
            return Ok(self
                .strict_replicated_doc_access
                .load(Ordering::Relaxed)
                .then(|| {
                    MergeOutcome::retryable_skip(
                        "replicated protected document is not yet registered in local ACP",
                    )
                }));
        }

        let permission = if frame.status == DELETED_STATUS {
            DocumentPermission::Delete
        } else {
            DocumentPermission::Update
        };
        let signer = frame.signer;
        let owed = if self.replicas_for(collection).is_some() {
            acp.replica_owed(&policy.id, &policy.resource_name, doc_id)
                .await
                .map_err(|e| MergeError::MergeFailed(format!("ACP owed lookup failed: {}", e)))?
        } else {
            acp::ReplicaOwed::default()
        };
        let canonical = signer.map(|signer| acp.canonical_actor(signer));
        // The provider has not seen a registration this node still owes, so
        // its answer about anybody but the DID it names is not the
        // provider's: the change waits.
        if let Some(registrant) = &owed.registration {
            if canonical.as_ref() != Some(&acp.canonical_actor(registrant)) {
                self.settle_owed_replica(acp.as_ref(), policy, doc_id).await;
                return Ok(Some(MergeOutcome::retryable_skip(
                    "a registration of this replicated document is still owed to the ACP provider",
                )));
            }
        }
        let allowed = match signer {
            Some(signer) => {
                self.signer_may(acp.as_ref(), signer.clone(), permission, policy, doc_id)
                    .await?
            }
            None => false,
        };
        let is_creator = canonical.is_some() && canonical == owed.creator;
        if is_creator && (owed.creator_grants || (owed.adopted && !allowed)) {
            self.settle_owed_replica(acp.as_ref(), policy, doc_id).await;
            return Ok(Some(MergeOutcome::retryable_skip(
                "the grants for this replicated document's creator are still owed to the ACP provider",
            )));
        }
        if allowed {
            return Ok(None);
        }
        Ok(Some(MergeOutcome::rejected(format!(
            "signer {} lacks {} permission on protected document {}",
            signer.map_or("unsigned", identity::Did::as_str),
            permission.as_str(),
            doc_id
        ))))
    }

    fn parks_refusals(&self, collection: &CollectionVersion) -> bool {
        self.strict()
            && collection.policy.is_some()
            && self
                .document_acp()
                .is_some_and(|acp| !acp.supports_transactions())
    }

    fn plans_replicas(&self, collection: &CollectionVersion) -> bool {
        self.replicas_for(collection).is_some()
    }

    async fn plan_replica(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
        is_genesis: bool,
        signer: Option<&str>,
    ) -> Result<ReplicaStep, MergeError> {
        let (Some(acp), Some(replicas), Some(policy)) = (
            self.document_acp(),
            self.replicas_for(collection),
            collection.policy.as_ref(),
        ) else {
            return Ok(ReplicaStep::Gate);
        };
        let registered = acp
            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| {
                MergeError::MergeFailed(format!("ACP registration lookup failed: {}", e))
            })?;
        if !is_genesis {
            return Ok(if registered {
                ReplicaStep::Gate
            } else {
                ReplicaStep::AfterGenesis
            });
        }
        let Some(creator) = signer.and_then(|signer| identity::Did::new(signer).ok()) else {
            return Ok(ReplicaStep::Gate);
        };
        let ownership = replicas.ownership();
        if ownership.is_own_actor(&collection.name, &creator) {
            return Ok(ReplicaStep::Gate);
        }
        match ownership
            .allows_foreign_registration(collection, &creator, acp.as_ref())
            .await
        {
            Ok(true) => {}
            Ok(false) => return Ok(ReplicaStep::Gate),
            Err(error) => {
                tracing::debug!(%doc_id, %error, "Could not establish replica signer membership");
                return Ok(ReplicaStep::Gate);
            }
        }
        let Some(registrant) = acp.own_actor() else {
            tracing::warn!(
                %doc_id,
                "The ACP provider names no actor of its own; the replicated document waits for a registration"
            );
            return Ok(ReplicaStep::Gate);
        };
        if !registered {
            // A gated command binds the parent from the signed body. Check its
            // authority when staging that exact command in the merge transaction.
            if policy.parent_field.is_none() {
                if let Err(error) = acp.command_authority(
                    &registrant,
                    acp::ProviderCommand::RegisterObject {
                        policy_id: &policy.id,
                        resource: &policy.resource_name,
                        object: doc_id,
                    },
                ) {
                    tracing::warn!(
                        %doc_id,
                        %error,
                        "The node cannot register a replicated document as its own actor; it waits for a registration"
                    );
                    return Ok(ReplicaStep::Gate);
                }
            }
            let grants = ownership.creator_grants(&collection.name, &creator, doc_id);
            return Ok(ReplicaStep::Register {
                creator,
                registrant,
                grants,
            });
        }
        let owed = acp
            .replica_owed(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| MergeError::MergeFailed(format!("ACP owed lookup failed: {}", e)))?;
        if owed.creator.is_some() || owed.registration.is_some() {
            return Ok(ReplicaStep::Gate);
        }
        let owner = acp
            .get_doc_owner(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| MergeError::MergeFailed(format!("ACP owner lookup failed: {}", e)))?
            .map(|owner| acp.canonical_actor(&owner));
        // Another actor holds the document (another node of this deployment
        // registered it first): its owner owes the creator's grants. Waiting
        // for them never widens anything; a refusal would be final.
        if owner.is_none() || owner == Some(acp.canonical_actor(&creator)) {
            return Ok(ReplicaStep::Gate);
        }
        let creator_writes = acp
            .check_doc_access(
                &Identity::Authenticated(creator.clone()),
                DocumentPermission::Update,
                &policy.id,
                &policy.resource_name,
                doc_id,
            )
            .await
            .map_err(|e| MergeError::MergeFailed(format!("ACP access check failed: {}", e)))?;
        Ok(if creator_writes {
            ReplicaStep::Gate
        } else {
            ReplicaStep::Adopt { creator }
        })
    }

    async fn on_protected_composite(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
        metadata: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        let Some(acp) = self.document_acp() else {
            return Ok(None);
        };
        let Some(policy) = &collection.policy else {
            return Ok(None);
        };
        if !self.strict_replicated_doc_access.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if metadata.allows_explicit_replay_for(&collection.collection_id) {
            return Ok(None);
        }
        let Some(identity) = self.local_identity.as_ref() else {
            return Ok(None);
        };

        let is_registered = acp
            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| {
                MergeError::MergeFailed(format!("ACP registration lookup failed: {}", e))
            })?;
        if !is_registered {
            return Ok(Some(MergeOutcome::retryable_skip(
                "replicated protected document is not yet registered in local ACP",
            )));
        }

        if self
            .node_reads(acp.as_ref(), identity, policy, doc_id)
            .await?
        {
            return Ok(None);
        }

        Ok(Some(MergeOutcome::retryable_skip(
            "replicated protected document is not yet readable by local node",
        )))
    }

    async fn on_encrypted_link(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
        metadata: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        let Some(acp) = self.document_acp() else {
            return Ok(None);
        };
        let Some(policy) = &collection.policy else {
            return Ok(None);
        };
        if !self.strict_replicated_doc_access.load(Ordering::Relaxed) {
            return Ok(None);
        }

        if metadata.allows_explicit_replay_for(&collection.collection_id) {
            tracing::info!(
                doc_id = %doc_id,
                sender_peer = metadata.sender_peer.unwrap_or(""),
                authorizer = metadata
                    .explicit_replay_authorization
                    .as_ref()
                    .map(|authorization| authorization.authorizer_did.as_str())
                    .unwrap_or(""),
                creator = metadata.effective_creator().unwrap_or(""),
                "Allowing encrypted merge via explicit replicator path"
            );
            return Ok(None);
        }

        let is_registered = acp
            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|e| {
                MergeError::MergeFailed(format!("ACP registration lookup failed: {}", e))
            })?;

        if is_registered {
            return Ok(None);
        }

        Ok(Some(MergeOutcome::retryable_skip(
            "encrypted replicated document is not yet registered in local ACP",
        )))
    }

    fn post_commit_action(
        &self,
        doc_id: &str,
        collection: &CollectionVersion,
        metadata: &BlockMetadata<'_>,
    ) -> Option<Box<dyn CompositePostCommitAction>> {
        // Only register a replicated document's owner on the receiving node under
        // strict (Vera) ACP, where cross-node access control is authoritative
        // via shared on-chain state. Under Local ACP we match Go: a replicated
        // document is NOT registered on the peer (unregistered == public), so the
        // peer does not gate it. Cross-node Local ACP gating was a Rust-only
        // feature; see also the removed relationship-propagation path.
        if !self.strict_replicated_doc_access.load(Ordering::Relaxed) {
            return None;
        }
        let acp = self.document_acp()?.clone();
        let policy = collection.policy.as_ref()?;
        if self.replicas_for(collection).is_some() {
            return Some(Box::new(SettleReplicaAction {
                acp,
                policy_id: policy.id.clone(),
                resource_name: policy.resource_name.clone(),
                doc_id: doc_id.to_string(),
            }));
        }
        if policy.parent_field.is_some() {
            return None;
        }
        let creator = metadata
            .explicit_replay_authorizer_for(&collection.collection_id)
            .or_else(|| metadata.effective_creator())?;
        let did = identity::Did::new(creator).ok()?;

        Some(Box::new(RegisterReplicatedDocAction {
            acp,
            did,
            policy_id: policy.id.clone(),
            resource_name: policy.resource_name.clone(),
            doc_id: doc_id.to_string(),
        }))
    }
}

pub struct AcpMergeHandler<S: Store, B: blockstore::Blockstore> {
    inner: Arc<DbMergeHandler<S, B>>,
    hook: Arc<AcpCompositeMergeHook>,
}

impl<S: Store, B: blockstore::Blockstore> AcpMergeHandler<S, B> {
    pub fn new(inner: Arc<DbMergeHandler<S, B>>) -> Self {
        let local_identity = inner
            .db
            .node_identity()
            .and_then(|identity| identity.did().ok().map(Identity::from));
        // Non-Send on wasm32, where the ACP traits it holds drop their `Send`
        // bounds for the single-threaded browser runtime.
        #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
        let hook = Arc::new(AcpCompositeMergeHook::new(local_identity));
        if let Some(replicas) = inner.db.replica_registration() {
            hook.set_replica_registration(Arc::clone(replicas));
        }
        inner.set_composite_merge_hook(hook.clone());
        Self { inner, hook }
    }

    pub fn with_document_acp(self, acp: Arc<dyn DocumentACP>) -> Self
    where
        S: 'static,
        B: 'static,
    {
        self.set_document_acp(acp);
        self
    }

    /// The hook decides with the provider as the database installed it, so an
    /// external provider is gated by the registrations creates owe it, and
    /// changes parked on its refusals are re-driven when this node changes
    /// their document's relationships.
    pub fn set_document_acp(&self, acp: Arc<dyn DocumentACP>)
    where
        S: 'static,
        B: 'static,
    {
        self.inner.db.set_document_acp(acp);
        if let Some(installed) = self.inner.db.document_acp() {
            self.hook.set_document_acp(installed.clone());
        }
        self.inner.watch_parked_acp();
    }

    pub fn set_strict_replicated_doc_access(&self, strict: bool) {
        self.hook.set_strict_replicated_doc_access(strict);
    }

    pub fn document_acp(&self) -> Option<&Arc<dyn DocumentACP>> {
        self.hook.document_acp()
    }

    pub fn strict_replicated_doc_access(&self) -> bool {
        self.hook
            .strict_replicated_doc_access
            .load(Ordering::Relaxed)
    }

    pub fn inner(&self) -> &Arc<DbMergeHandler<S, B>> {
        &self.inner
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> MergeHandler for AcpMergeHandler<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + 'static,
{
    type Error = AcpMergeError;

    async fn validate_authorization(
        &self,
        authorization: Option<&ExplicitReplayAuthorization>,
        block: &MergeBlock,
    ) -> Result<(), Self::Error> {
        self.inner
            .validate_authorization(authorization, block)
            .await
    }

    async fn recover_block_metadata(
        &self,
        cid: &Cid,
        block_data: &[u8],
    ) -> Result<Option<RecoveredBlockMetadata>, Self::Error> {
        self.inner.recover_block_metadata(cid, block_data).await
    }

    async fn handle_block(
        &self,
        cid: &Cid,
        block_data: &[u8],
        metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, Self::Error> {
        self.inner.handle_block(cid, block_data, metadata).await
    }

    async fn handle_block_batch(
        &self,
        blocks: &[MergeBlock],
    ) -> Vec<Result<MergeOutcome, Self::Error>> {
        self.inner.handle_block_batch(blocks).await
    }
}
