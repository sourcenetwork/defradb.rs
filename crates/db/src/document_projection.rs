use std::sync::{Arc, Weak};

use async_trait::async_trait;
use datastore::NamespaceView;
use identity::Did;
use storage::corekv::{MaybeSendSync, OnCommit, Result, Store};
use storage::namespace::Namespace;

/// Trusted context for projecting an authorized document mutation. The
/// provider is the node's configured document ACP, and the view belongs to
/// the same transaction as the mutation. Native ACP remains authoritative.
pub struct DocumentMutationContext<'a> {
    pub view: &'a NamespaceView,
    pub document_acp: &'a Arc<dyn acp::DocumentACP>,
    pub node_did: &'a Did,
    notifier: &'a dyn AccessChangeNotifier,
    nac: Option<&'a dyn crate::nac::NacManagerApi>,
    rules: &'a [acp::projection::Rule],
    projected: Option<(&'a str, &'a str)>,
    reconciling: bool,
}

impl DocumentMutationContext<'_> {
    pub async fn registration_pending(
        &self,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> Result<bool> {
        let view = self.view.sibling(Namespace::Acpstore);
        acp::PendingRegistrations::new(&view)
            .get(policy, resource, doc)
            .await
            .map(|registration| registration.is_some())
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))
    }

    pub async fn projected_actor_relationship(
        &self,
        policy: &str,
        resource: &str,
        doc: &str,
        relation: &str,
        target: &Did,
    ) -> Result<bool> {
        let view = self.view.sibling(Namespace::Acpstore);
        if self.document_acp.supports_transactions() {
            let store = acp::zanzibar::store::TransactionZanzibarStore::new(view);
            return acp::ZanzibarStore::has_relationship(
                &store,
                policy,
                resource,
                doc,
                relation,
                &acp::Subject::entity(target.clone()),
            )
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()));
        }
        if self.registration_pending(policy, resource, doc).await? {
            return Ok(false);
        }
        let actor = self
            .view
            .projection_actor()
            .unwrap_or(self.node_did.as_str());
        let bearer = self
            .view
            .projection_context()
            .and_then(|context| context.bearer_token_for(actor));
        let request = serde_json::to_vec(&(
            actor,
            "relationship",
            policy,
            resource,
            doc,
            bearer,
            relation,
            target.as_str(),
        ))
        .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
        let Some(response) = self
            .view
            .projection_inputs()
            .and_then(|inputs| inputs.get(&request))
        else {
            return Err(storage::corekv::Error::ProjectionAcpInput(
                storage::corekv::errors::ProjectionAcpInput::new(request),
            ));
        };
        serde_json::from_slice(&response).map_err(|error| {
            storage::corekv::Error::Other(format!("invalid prepared relationship: {error}"))
        })
    }

    pub async fn projected_owner(
        &self,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
    ) -> Result<Option<Did>> {
        let view = self.view.sibling(Namespace::Acpstore);
        if let Some(pending) = acp::PendingRegistrations::new(&view)
            .get(policy_id, resource, doc_id)
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
        {
            return Ok(Some(pending.creator));
        }
        if let Some(scoped) = self.document_acp.bind_transaction(view) {
            return scoped
                .get_doc_owner(policy_id, resource, doc_id)
                .await
                .map_err(|error| storage::corekv::Error::Other(error.to_string()));
        }
        let actor = self
            .view
            .projection_actor()
            .unwrap_or(self.node_did.as_str());
        let bearer = self
            .view
            .projection_context()
            .and_then(|context| context.bearer_token_for(actor));
        let request = serde_json::to_vec(&(actor, "owner", policy_id, resource, doc_id, bearer))
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
        let Some(response) = self
            .view
            .projection_inputs()
            .and_then(|inputs| inputs.get(&request))
        else {
            return Err(storage::corekv::Error::ProjectionAcpInput(
                storage::corekv::errors::ProjectionAcpInput::new(request),
            ));
        };
        serde_json::from_slice(&response).map_err(|error| {
            storage::corekv::Error::Other(format!("invalid prepared document owner: {error}"))
        })
    }

    pub async fn change_projected_relationship(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: acp::Subject,
        delete: bool,
    ) -> Result<bool> {
        if !self.reconciling && self.projected != Some((collection, doc_id)) {
            return Err(storage::corekv::Error::ProjectionPermissionDenied);
        }
        let actor = match self.view.projection_actor() {
            Some(actor) => {
                Did::new(actor).map_err(|error| storage::corekv::Error::Other(error.to_string()))?
            }
            None => self.node_did.clone(),
        };
        let unavailable = || {
            storage::corekv::Error::Other(
                "transaction-bound node authorization is unavailable".into(),
            )
        };
        let permission = if delete {
            acp::NodePermission::DacRelationDelete
        } else {
            acp::NodePermission::DacRelationAdd
        };
        let granted = self
            .nac
            .ok_or_else(unavailable)?
            .check_bound_permission_in_txn(
                self.view.sibling(Namespace::Acpstore),
                &actor,
                permission,
                true,
            )
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
            .ok_or_else(unavailable)?;
        if !granted {
            return Err(storage::corekv::Error::ProjectionPermissionDenied);
        }
        let collection_name = collection;
        let system = self.view.sibling(Namespace::Systemstore);
        let collection =
            crate::collection::loader::load_collection_from_systemstore(&system, collection)
                .await
                .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
                .ok_or_else(|| {
                    storage::corekv::Error::Other("projection collection unavailable".into())
                })?;
        let binding =
            collection.schema().policy.as_ref().ok_or_else(|| {
                storage::corekv::Error::Other("projection policy unavailable".into())
            })?;
        if relation == "owner"
            || !acp::projection::in_scope(
                self.rules,
                collection_name,
                &binding.resource_name,
                relation,
                &target,
            )
        {
            return Err(storage::corekv::Error::ProjectionPermissionDenied);
        }
        let acp_view = self.view.sibling(Namespace::Acpstore);
        let policy_store = acp::zanzibar::store::TransactionZanzibarStore::new(acp_view.clone());
        let policy = acp::ZanzibarStore::get_policy(&policy_store, &binding.id)
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
            .ok_or_else(|| storage::corekv::Error::Other("projection policy unavailable".into()))?;
        if policy
            .get_relation(&binding.resource_name, relation)
            .is_none()
        {
            return Err(storage::corekv::Error::ProjectionPermissionDenied);
        }
        let Some(acp) = self.document_acp.bind_transaction(acp_view.clone()) else {
            let pending = acp::PendingRegistrations::new(&acp_view)
                .get(&binding.id, &binding.resource_name, doc_id)
                .await
                .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
            if pending.as_ref().is_some_and(|pending| {
                self.document_acp.canonical_actor(&pending.creator)
                    != self.document_acp.canonical_actor(&actor)
            }) {
                return Err(storage::corekv::Error::ProjectionPermissionDenied);
            }
            let managers = policy
                .get_managers_for_relation(&binding.resource_name, relation)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let op = if delete {
                acp::OwedOp::Delete
            } else {
                acp::OwedOp::Add
            };
            let prepared = if pending.is_none() {
                let bearer = self
                    .view
                    .projection_context()
                    .and_then(|context| context.bearer_token_for(actor.as_str()));
                let subject = acp::OwedSubject::try_from(target.clone())
                    .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
                let request = serde_json::to_vec(&(
                    actor.as_str(),
                    "stage",
                    &binding.id,
                    &binding.resource_name,
                    doc_id,
                    bearer,
                    relation,
                    subject,
                    op,
                    &managers,
                ))
                .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
                let Some(authority) = self
                    .view
                    .projection_inputs()
                    .and_then(|inputs| inputs.authority(&request))
                else {
                    return Err(storage::corekv::Error::ProjectionAcpInput(
                        storage::corekv::errors::ProjectionAcpInput::new(request),
                    ));
                };
                Some(authority)
            } else {
                None
            };
            let prepared_owner = prepared.as_ref().and_then(|authority| {
                authority
                    .as_any()
                    .downcast_ref::<crate::txn::ProjectionAuthority>()
            });
            if prepared.is_some() && prepared_owner.is_none() {
                return Err(storage::corekv::Error::ProjectionPermissionDenied);
            }
            let provider = if let Some(prepared) = prepared_owner {
                prepared.0.outbox_provider()
            } else {
                match self.view.projection_context() {
                    Some(context) => self.document_acp.bind_request(context).ok_or_else(|| {
                        storage::corekv::Error::Other(
                            "request-bound document authorization is unavailable".into(),
                        )
                    })?,
                    None => Arc::clone(self.document_acp),
                }
            };
            let staged = self.view.projection_staged().ok_or_else(|| {
                storage::corekv::Error::Other("projection transaction outbox is unavailable".into())
            })?;
            let outbox = provider
                .transaction_outbox(&acp_view, acp::StagedCount::from_shared(staged))
                .ok_or_else(|| {
                    storage::corekv::Error::Other(
                        "projection transaction outbox is unavailable".into(),
                    )
                })?;
            let first = outbox.staged() == 0;
            let txn = self
                .view
                .projection_context()
                .and_then(|context| context.txn())
                .map(|id| id.0);
            let change = acp::StageRelationship {
                txn,
                actor: &actor,
                subject: target,
                policy_id: &binding.id,
                resource: &binding.resource_name,
                object: doc_id,
                relation,
                managing_relations: &managers,
                op,
            };
            match prepared_owner {
                Some(prepared) => outbox.stage_prepared_owner(change, &prepared.0).await,
                None => outbox.stage(change).await,
            }
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
            if first {
                let provider = Arc::clone(self.document_acp);
                self.view
                    .on_commit(OnCommit::Async(Box::new(move || {
                        Box::pin(async move {
                            let scope =
                                txn.map_or(acp::OwedScope::All, acp::OwedScope::Transaction);
                        match workers::timeout(
                            std::time::Duration::from_secs(10),
                            provider.settle_owed_changes(scope),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(error)) => {
                                tracing::warn!(%error, "projection relationship settlement failed");
                            }
                            Err(error) => {
                                tracing::warn!(%error, "projection relationship settlement did not finish");
                            }
                            }
                        })
                    })))
                    .await?;
            }
            return Ok(true);
        };
        if !acp
            .is_doc_registered(&binding.id, &binding.resource_name, doc_id)
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
        {
            return Err(storage::corekv::Error::ProjectionPermissionDenied);
        }
        let result = if delete {
            acp.delete_relationship(
                &actor,
                target,
                &binding.id,
                &binding.resource_name,
                doc_id,
                relation,
                &[],
            )
            .await
        } else {
            acp.add_relationship(
                &actor,
                target,
                &binding.id,
                &binding.resource_name,
                doc_id,
                relation,
                &[],
            )
            .await
        };
        result.map_err(|error| storage::corekv::Error::Other(error.to_string()))
    }
    /// Stage a source edge as the actor that caused this projection. Startup,
    /// replication and any other actorless operation cannot mint or revoke
    /// a source. Native NAC and the document share the same transaction.
    /// An exactly present add or exactly absent delete changes nothing and
    /// returns false without the actor's authority; the read that found it
    /// unchanged is validated at commit. A changing edge needs the actor's own
    /// authority and never the node's.
    pub async fn change_source_permission(
        &self,
        permission: acp::NodePermission,
        parent_source: Option<&str>,
        subject_kind: &str,
        subject: &str,
        delete: bool,
    ) -> Result<bool> {
        let actor = self
            .view
            .projection_actor()
            .ok_or(storage::corekv::Error::ProjectionPermissionDenied)?;
        let actor =
            Did::new(actor).map_err(|error| storage::corekv::Error::Other(error.to_string()))?;
        let unavailable = || {
            storage::corekv::Error::Other(
                "transaction-bound node authorization is unavailable".into(),
            )
        };
        let nac = self.nac.ok_or_else(unavailable)?;
        let change = crate::nac::SourcePermissionChange {
            permission,
            parent_source,
            subject_kind,
            subject,
            delete,
        };
        let unchanged = nac
            .source_permission_matches_in_txn(self.view.sibling(Namespace::Acpstore), &change)
            .await
            .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
            .ok_or_else(unavailable)?;
        if unchanged {
            return Ok(false);
        }
        nac.change_source_permission_in_txn(self.view.sibling(Namespace::Acpstore), &actor, &change)
            .await
            .map_err(|error| match error {
                crate::nac::Error::PermissionDenied => {
                    storage::corekv::Error::ProjectionPermissionDenied
                }
                other => storage::corekv::Error::Other(other.to_string()),
            })?
            .ok_or_else(unavailable)
    }

    /// Queue the ordinary document invalidation for a changed ACP relation.
    /// A transaction that aborts discards this callback with the relation.
    pub async fn notify_access_changed(&self, collection: &str, doc_id: &str) -> Result<()> {
        self.notifier.stage(self.view, collection, doc_id).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
trait AccessChangeNotifier: MaybeSendSync {
    async fn stage(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()>;
}

struct DatabaseAccessNotifier<S: Store + 'static> {
    database: Weak<crate::database::DB<S>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> AccessChangeNotifier for DatabaseAccessNotifier<S> {
    async fn stage(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()> {
        let system = view.sibling(Namespace::Systemstore);
        let collection =
            crate::collection::loader::load_collection_from_systemstore(&system, collection)
                .await
                .map_err(|error| storage::corekv::Error::Other(error.to_string()))?
                .ok_or_else(|| {
                    storage::corekv::Error::Other("missing access-change collection".into())
                })?;
        let collection_id = collection.collection_id().to_owned();
        let doc_id = doc_id.to_owned();
        let database = self.database.clone();
        view.on_commit(OnCommit::Async(Box::new(move || {
            Box::pin(async move {
                let Some(database) = database.upgrade() else {
                    return;
                };
                let Some(bus) = database.event_bus() else {
                    return;
                };
                match crate::block::reader::read_latest_composite_block(&database, &doc_id).await {
                    Ok(result) => bus.publish(events::Message::update(events::Update::new(
                        doc_id,
                        result.cid,
                        collection_id,
                        result.block,
                        false,
                        false,
                    ))),
                    Err(error) => tracing::warn!(
                        doc_id,
                        error,
                        "failed to publish document update after ACP change"
                    ),
                }
            })
        })))
        .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DocumentMutationProjector: MaybeSendSync {
    fn genesis_reader_field(&self, _collection: &str, _policy: &str, _resource: &str) -> Option<&'static str> { None }

    fn projection_rules(&self) -> &[acp::projection::Rule] {
        &[]
    }
    async fn reconcile(&self, context: &DocumentMutationContext<'_>) -> Result<()>;
    async fn project_related(
        &self,
        _context: &DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> Result<()> {
        Ok(())
    }
    /// The host changed a source edge using the same transaction; no document
    /// writer or transport can manufacture this notification.
    async fn source_changed(&self, _context: &DocumentMutationContext<'_>) -> Result<()> {
        Ok(())
    }
    async fn project(
        &self,
        context: &DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> Result<()>;
}

pub(crate) struct BoundProjection {
    pub(crate) projector: Arc<dyn DocumentMutationProjector>,
    pub(crate) document_acp: Arc<dyn acp::DocumentACP>,
    pub(crate) node_did: Did,
    notifier: Box<dyn AccessChangeNotifier>,
    nac: Option<Arc<dyn crate::nac::NacManagerApi>>,
}

impl BoundProjection {
    pub(crate) fn new<S: Store + 'static>(
        projector: Arc<dyn DocumentMutationProjector>,
        document_acp: Arc<dyn acp::DocumentACP>,
        node_did: Did,
        database: Weak<crate::database::DB<S>>,
    ) -> Self {
        let nac = database.upgrade().and_then(|db| db.nac_manager());
        Self {
            projector,
            document_acp,
            node_did,
            notifier: Box::new(DatabaseAccessNotifier { database }),
            nac,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl datastore::DocumentMutationProjector for BoundProjection {
    fn genesis_reader_field(&self, collection: &str, policy: &str, resource: &str) -> Option<&'static str> { self.projector.genesis_reader_field(collection, policy, resource) }

    async fn source_changed(&self, view: &NamespaceView) -> Result<()> {
        self.projector
            .source_changed(&DocumentMutationContext {
                view,
                document_acp: &self.document_acp,
                node_did: &self.node_did,
                notifier: self.notifier.as_ref(),
                nac: self.nac.as_deref(),
                rules: self.projector.projection_rules(),
                projected: None,
                reconciling: false,
            })
            .await
    }

    async fn reconcile(&self, view: &NamespaceView) -> Result<()> {
        self.projector
            .reconcile(&DocumentMutationContext {
                view,
                document_acp: &self.document_acp,
                node_did: &self.node_did,
                notifier: self.notifier.as_ref(),
                nac: self.nac.as_deref(),
                rules: self.projector.projection_rules(),
                projected: None,
                reconciling: true,
            })
            .await
    }

    async fn project(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()> {
        self.projector
            .project(
                &DocumentMutationContext {
                    view,
                    document_acp: &self.document_acp,
                    node_did: &self.node_did,
                    notifier: self.notifier.as_ref(),
                    nac: self.nac.as_deref(),
                    rules: self.projector.projection_rules(),
                    projected: Some((collection, doc_id)),
                    reconciling: false,
                },
                collection,
                doc_id,
            )
            .await?;
        self.projector
            .project_related(
                &DocumentMutationContext {
                    view,
                    document_acp: &self.document_acp,
                    node_did: &self.node_did,
                    notifier: self.notifier.as_ref(),
                    nac: self.nac.as_deref(),
                    rules: self.projector.projection_rules(),
                    projected: Some((collection, doc_id)),
                    reconciling: true,
                },
                collection,
                doc_id,
            )
            .await
    }
}
