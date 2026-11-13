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
    pub document_acp: &'a dyn acp::DocumentACP,
    pub node_did: &'a Did,
    notifier: &'a dyn AccessChangeNotifier,
    nac: Option<&'a dyn crate::nac::NacManagerApi>,
}

impl DocumentMutationContext<'_> {
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
    async fn reconcile(&self, context: &DocumentMutationContext<'_>) -> Result<()>;
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

impl BoundProjection {
    /// A projector may grant or revoke NAC tuples in the transaction it runs
    /// in, so the edge snapshot is rebuilt once that transaction commits.
    async fn refresh_nac_edge(&self, view: &NamespaceView) -> Result<()> {
        match &self.nac {
            Some(nac) => nac
                .refresh_edge_on_commit_of(view)
                .await
                .map_err(|error| storage::corekv::Error::Other(error.to_string())),
            None => Ok(()),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl datastore::DocumentMutationProjector for BoundProjection {
    async fn source_changed(&self, view: &NamespaceView) -> Result<()> {
        self.refresh_nac_edge(view).await?;
        self.projector
            .source_changed(&DocumentMutationContext {
                view,
                document_acp: self.document_acp.as_ref(),
                node_did: &self.node_did,
                notifier: self.notifier.as_ref(),
                nac: self.nac.as_deref(),
            })
            .await
    }

    async fn reconcile(&self, view: &NamespaceView) -> Result<()> {
        self.refresh_nac_edge(view).await?;
        self.projector
            .reconcile(&DocumentMutationContext {
                view,
                document_acp: self.document_acp.as_ref(),
                node_did: &self.node_did,
                notifier: self.notifier.as_ref(),
                nac: self.nac.as_deref(),
            })
            .await
    }

    async fn project(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()> {
        self.refresh_nac_edge(view).await?;
        self.projector
            .project(
                &DocumentMutationContext {
                    view,
                    document_acp: self.document_acp.as_ref(),
                    node_did: &self.node_did,
                    notifier: self.notifier.as_ref(),
                    nac: self.nac.as_deref(),
                },
                collection,
                doc_id,
            )
            .await
    }
}
