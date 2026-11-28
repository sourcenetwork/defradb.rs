//! HTTP's DocumentAcpOperations over the core's document relationship
//! operations. A request in a transaction runs in it.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{DocRelationshipChange, DocumentAcpOperations};
use storage::corekv::Store;

/// The core's document relationship operations, serving one request once
/// bound to it.
pub struct DocumentAcpAdapter<S>
where
    S: Store + 'static,
{
    relationships: db::DocumentRelationships<S>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> DocumentAcpAdapter<S>
where
    S: Store + 'static,
{
    /// An adapter over `registry`, deciding through `acp`.
    pub fn new(
        registry: Arc<db::DbTransactionRegistry<S>>,
        acp: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self {
            relationships: db::DocumentRelationships::new(registry, acp),
            context: None,
        }
    }

    /// Create an Arc-wrapped adapter.
    pub fn new_arc(
        registry: Arc<db::DbTransactionRegistry<S>>,
        acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn DocumentAcpOperations> {
        Arc::new(Self::new(registry, acp))
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

/// The core reads an empty identity as none.
fn identity(requestor: Option<&identity::Did>) -> String {
    requestor.map(ToString::to_string).unwrap_or_default()
}

fn relationship<'a>(
    target: &'a str,
    collection: &'a str,
    doc_id: &'a str,
    relation: &'a str,
) -> db::DocRelationship<'a> {
    db::DocRelationship {
        collection,
        doc_id,
        relation,
        target,
    }
}

fn text(error: db::DocAcpError) -> String {
    error.to_string()
}

#[async_trait]
impl<S> DocumentAcpOperations for DocumentAcpAdapter<S>
where
    S: Store + 'static,
{
    /// A delegation authenticates the caller the node's document ACP
    /// verifies it for, as on an in-process node, so a caller whose key this
    /// server does not hold acts under its own delegation.
    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<Result<acp::AuthenticatedCaller, String>> {
        self.relationships.caller_of_delegation(token)
    }

    fn supports_transactional_relationships(&self) -> bool {
        self.relationships.changes_in_transactions()
    }

    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn DocumentAcpOperations>> {
        Some(Arc::new(Self {
            relationships: self.relationships.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn change_doc_relationship(
        &self,
        requestor: Option<&identity::Did>,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        delete: bool,
    ) -> Result<DocRelationshipChange, String> {
        let change = self
            .relationships
            .change_as(
                &self.context(),
                &identity(requestor),
                relationship(target_actor, collection, doc_id, relation),
                delete,
            )
            .await
            .map_err(text)?;
        Ok(match change {
            db::RelationshipChange::Applied { existed } => {
                DocRelationshipChange::Applied { existed }
            }
            db::RelationshipChange::Staged {
                seq,
                authority_checked,
            } => DocRelationshipChange::Staged {
                seq,
                authority_checked,
            },
        })
    }

    async fn check_doc_access(
        &self,
        actor: &identity::Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool, String> {
        self.relationships
            .check_doc_access_as(
                &self.context(),
                actor,
                permission,
                policy_id,
                resource_name,
                doc_id,
            )
            .await
            .map_err(text)
    }

    async fn add_doc_relationship(
        &self,
        requestor: Option<&identity::Did>,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        self.relationships
            .add_as(
                &self.context(),
                &identity(requestor),
                relationship(target_actor, collection, doc_id, relation),
            )
            .await
            // The core answers whether it existed already; the trait, whether it is new.
            .map(|existed| !existed)
            .map_err(text)
    }

    async fn observe_doc_actor_relationship(
        &self,
        requestor: &identity::Did,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool, String> {
        self.relationships
            .observe_as(
                &self.context(),
                requestor.as_str(),
                relationship(target, collection, doc_id, relation),
            )
            .await
            .map_err(text)
    }

    async fn list_doc_relationships(
        &self,
        requestor: &identity::Did,
        collection: &str,
        doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>, String> {
        self.relationships
            .list_as(&self.context(), requestor.as_str(), collection, doc_id)
            .await
            .map_err(text)
    }

    async fn delete_doc_relationship(
        &self,
        requestor: Option<&identity::Did>,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        self.relationships
            .delete_as(
                &self.context(),
                &identity(requestor),
                relationship(target_actor, collection, doc_id, relation),
            )
            .await
            .map_err(text)
    }
}
