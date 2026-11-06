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
}

impl DocumentMutationContext<'_> {
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
}

impl BoundProjection {
    pub(crate) fn new<S: Store + 'static>(
        projector: Arc<dyn DocumentMutationProjector>,
        document_acp: Arc<dyn acp::DocumentACP>,
        node_did: Did,
        database: Weak<crate::database::DB<S>>,
    ) -> Self {
        Self {
            projector,
            document_acp,
            node_did,
            notifier: Box::new(DatabaseAccessNotifier { database }),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl datastore::DocumentMutationProjector for BoundProjection {
    async fn reconcile(&self, view: &NamespaceView) -> Result<()> {
        self.projector
            .reconcile(&DocumentMutationContext {
                view,
                document_acp: self.document_acp.as_ref(),
                node_did: &self.node_did,
                notifier: self.notifier.as_ref(),
            })
            .await
    }

    async fn project(&self, view: &NamespaceView, collection: &str, doc_id: &str) -> Result<()> {
        self.projector
            .project(
                &DocumentMutationContext {
                    view,
                    document_acp: self.document_acp.as_ref(),
                    node_did: &self.node_did,
                    notifier: self.notifier.as_ref(),
                },
                collection,
                doc_id,
            )
            .await
    }
}
