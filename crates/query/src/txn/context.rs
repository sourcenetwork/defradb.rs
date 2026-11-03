//! Transaction context trait.

use acp::error::Result as AcpResult;
use acp::DocumentACP;
use acp::DocumentPermission;
use acp::Identity;
use defra_core::RequestContext;
use identity::Did;
use std::sync::Arc;
use storage::corekv::MaybeSendSync;

use crate::fetcher::CollectionProvider;
use crate::fetcher::DocFetcher;
use crate::mutator::DocMutator;

/// A transaction's view of the registrations its creates owe an external ACP
/// provider (see [`acp::pending`]). The owed records are the transaction's own
/// writes, so its statement savepoints and its rollback cover them and nothing
/// is copied here: a lookup is one point read through the transaction, and the
/// post-commit hooks that settle the records stay attached to the transaction
/// that owns the document writes. Remote ACP updates are not atomic with
/// storage; the records only keep each owed document readable by its creator
/// alone until the provider confirms.
#[derive(Clone, Default)]
pub struct AcpRegistrationProjection {
    pending: Option<acp::PendingRegistrations>,
}

impl AcpRegistrationProjection {
    /// Read owed registrations through the transaction behind `pending`.
    pub fn new(pending: acp::PendingRegistrations) -> Self {
        Self {
            pending: Some(pending),
        }
    }

    /// A projection with no transaction behind it: nothing is owed through it.
    pub fn detached() -> Self {
        Self::default()
    }

    /// A detached projection to share. It is not `Sync` in the single-threaded
    /// browser build, where nothing shares it across threads.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub fn detached_shared() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::detached())
    }

    /// The creator a registration is owed to, as this transaction sees it.
    pub async fn owed_creator(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> AcpResult<Option<Did>> {
        let Some(pending) = &self.pending else {
            return Ok(None);
        };
        Ok(pending
            .get(policy_id, resource_name, doc_id)
            .await?
            .map(|owed| owed.creator))
    }
}

/// Only an external provider is owed registrations; a transactional one
/// registers inside the create's transaction, so nothing is read for it.
async fn owed_creator(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    policy_id: &str,
    resource_name: &str,
    doc_id: &str,
) -> AcpResult<Option<Did>> {
    match projection {
        Some(projection) if !acp.supports_transactions() => {
            projection
                .owed_creator(policy_id, resource_name, doc_id)
                .await
        }
        _ => Ok(None),
    }
}

fn is_creator(identity: &Identity, creator: &Did) -> bool {
    matches!(identity, Identity::Authenticated(did) if did == creator)
}

/// Check document registration using an explicit request/transaction projection.
pub async fn is_doc_registered_with_projection(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    policy_id: &str,
    resource_name: &str,
    doc_id: &str,
) -> AcpResult<bool> {
    if owed_creator(acp, projection, policy_id, resource_name, doc_id)
        .await?
        .is_some()
    {
        return Ok(true);
    }
    acp.is_doc_registered(policy_id, resource_name, doc_id)
        .await
}

/// Resolve registration state from attempt-local projection, the live
/// transaction provider, or an exact request memo for external providers.
pub async fn is_doc_registered_with_prepared_inputs(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    identity: &Identity,
    policy_id: &str,
    resource_name: &str,
    doc_id: &str,
    inputs: Option<&crate::prepared::PreparedInputMemo>,
) -> crate::error::Result<bool> {
    if owed_creator(acp, projection, policy_id, resource_name, doc_id)
        .await
        .map_err(|error| crate::error::QueryError::execution(error.to_string()))?
        .is_some()
    {
        return Ok(true);
    }
    let Some(inputs) = inputs.filter(|_| !acp.supports_transactions()) else {
        return is_doc_registered_with_projection(
            acp,
            projection,
            policy_id,
            resource_name,
            doc_id,
        )
        .await
        .map_err(|error| crate::error::QueryError::execution(error.to_string()));
    };
    let bearer = identity
        .did()
        .and_then(|did| acp.bearer_token_for(did.as_str()));
    let exact_request = serde_json::to_vec(&(
        identity.did().map(Did::as_str).unwrap_or("anonymous"),
        "registered",
        policy_id,
        resource_name,
        doc_id,
        bearer,
    ))
    .map_err(|error| crate::error::QueryError::execution(error.to_string()))?;
    let request = crate::prepared::PreparedInputRequest::new(
        crate::prepared::PreparedInputKind::AcpDecision,
        exact_request,
    );
    let Some(response) = inputs.get(&request) else {
        return Err(crate::error::QueryError::NeedsInput(request));
    };
    serde_json::from_slice(&response).map_err(|error| {
        crate::error::QueryError::execution(format!("cached ACP response is invalid: {error}"))
    })
}

/// Check document access using an explicit request/transaction projection.
pub async fn check_doc_access_with_projection(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    identity: &Identity,
    permission: DocumentPermission,
    policy_id: &str,
    resource_name: &str,
    doc_id: &str,
) -> AcpResult<bool> {
    if let Some(creator) = owed_creator(acp, projection, policy_id, resource_name, doc_id).await? {
        return Ok(is_creator(identity, &creator));
    }
    acp.check_doc_access(identity, permission, policy_id, resource_name, doc_id)
        .await
}

/// Resolve a document permission through the request memo when running inside
/// an engine retry attempt. A cache miss exits the attempt so the caller can
/// ask the external ACP provider and re-enter with the exact answer.
#[allow(clippy::too_many_arguments)]
pub async fn check_doc_access_with_prepared_inputs(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    identity: &Identity,
    permission: DocumentPermission,
    policy_id: &str,
    resource_name: &str,
    doc_id: &str,
    inputs: Option<&crate::prepared::PreparedInputMemo>,
) -> crate::error::Result<bool> {
    // A registration this transaction owes overrides remote state, while
    // transaction-capable providers stay live so their reads participate in OCC.
    if let Some(creator) = owed_creator(acp, projection, policy_id, resource_name, doc_id)
        .await
        .map_err(|error| crate::error::QueryError::execution(error.to_string()))?
    {
        return Ok(is_creator(identity, &creator));
    }
    let Some(inputs) = inputs.filter(|_| !acp.supports_transactions()) else {
        return check_doc_access_with_projection(
            acp,
            projection,
            identity,
            permission,
            policy_id,
            resource_name,
            doc_id,
        )
        .await
        .map_err(|error| crate::error::QueryError::execution(error.to_string()));
    };
    let bearer = identity
        .did()
        .and_then(|did| acp.bearer_token_for(did.as_str()));
    let exact_request = serde_json::to_vec(&(
        identity.did().map(Did::as_str).unwrap_or("anonymous"),
        permission.as_str(),
        policy_id,
        resource_name,
        doc_id,
        bearer,
    ))
    .map_err(|error| crate::error::QueryError::execution(error.to_string()))?;
    let request = crate::prepared::PreparedInputRequest::new(
        crate::prepared::PreparedInputKind::AcpDecision,
        exact_request,
    );
    let Some(response) = inputs.get(&request) else {
        return Err(crate::error::QueryError::NeedsInput(request));
    };
    serde_json::from_slice(&response).map_err(|error| {
        crate::error::QueryError::execution(format!("cached ACP response is invalid: {error}"))
    })
}

/// Resolve shared object/branchable read permissions from the request memo while
/// retaining live transaction reads and attempt-local projected registrations.
#[allow(clippy::too_many_arguments)]
pub async fn check_doc_read_access_with_prepared_inputs(
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    identity: &Identity,
    policy_id: &str,
    resource_name: &str,
    collection_id: &str,
    is_branchable: bool,
    doc_id: &str,
    inputs: Option<&crate::prepared::PreparedInputMemo>,
) -> crate::error::Result<bool> {
    check_doc_read_access_as(
        &RequestContext::ambient(),
        acp,
        projection,
        identity,
        policy_id,
        resource_name,
        collection_id,
        is_branchable,
        doc_id,
        inputs,
    )
    .await
}

/// [`check_doc_read_access_with_prepared_inputs`] for the request `context`
/// serves: an authenticated reader whose node access grants it `bypass-dac`
/// reads every document.
#[allow(clippy::too_many_arguments)]
pub async fn check_doc_read_access_as(
    context: &RequestContext,
    acp: &dyn DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    identity: &Identity,
    policy_id: &str,
    resource_name: &str,
    collection_id: &str,
    is_branchable: bool,
    doc_id: &str,
    inputs: Option<&crate::prepared::PreparedInputMemo>,
) -> crate::error::Result<bool> {
    use acp::read_access::DocAccess;

    if identity.is_authenticated() && context.dac_bypass() {
        return Ok(true);
    }
    let Some(inputs) = inputs.filter(|_| !acp.supports_transactions()) else {
        let checker = crate::txn::OverlayChecker {
            acp,
            projection,
            identity,
        };
        return acp::read_access::check_doc_read_access(
            &checker,
            policy_id,
            resource_name,
            collection_id,
            is_branchable,
            doc_id,
        )
        .await
        .map_err(|error| crate::error::QueryError::execution(error.to_string()));
    };

    async fn access_for_object(
        acp: &dyn DocumentACP,
        projection: Option<&AcpRegistrationProjection>,
        identity: &Identity,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        inputs: &crate::prepared::PreparedInputMemo,
    ) -> crate::error::Result<DocAccess> {
        if let Some(creator) = owed_creator(acp, projection, policy_id, resource_name, object_id)
            .await
            .map_err(|error| crate::error::QueryError::execution(error.to_string()))?
        {
            return Ok(DocAccess {
                has_access: is_creator(identity, &creator),
                explicit: true,
            });
        }

        let bearer = identity
            .did()
            .and_then(|did| acp.bearer_token_for(did.as_str()));
        let request_bytes = serde_json::to_vec(&(
            identity.did().map(Did::as_str).unwrap_or("anonymous"),
            "registered",
            policy_id,
            resource_name,
            object_id,
            bearer,
        ))
        .map_err(|error| crate::error::QueryError::execution(error.to_string()))?;
        let request = crate::prepared::PreparedInputRequest::new(
            crate::prepared::PreparedInputKind::AcpDecision,
            request_bytes,
        );
        let Some(response) = inputs.get(&request) else {
            return Err(crate::error::QueryError::NeedsInput(request));
        };
        let registered = serde_json::from_slice::<bool>(&response)
            .map_err(|error| crate::error::QueryError::execution(error.to_string()))?;
        if !registered {
            return Ok(DocAccess {
                has_access: acp.unregistered_documents_are_public(),
                explicit: false,
            });
        }
        let allowed = check_doc_access_with_prepared_inputs(
            acp,
            projection,
            identity,
            DocumentPermission::Read,
            policy_id,
            resource_name,
            object_id,
            Some(inputs),
        )
        .await?;
        Ok(DocAccess {
            has_access: allowed,
            explicit: true,
        })
    }

    if !doc_id.is_empty() {
        let object = access_for_object(
            acp,
            projection,
            identity,
            policy_id,
            resource_name,
            doc_id,
            inputs,
        )
        .await?;
        if object.explicit || !is_branchable {
            return Ok(object.has_access);
        }
    }
    if is_branchable {
        let collection = access_for_object(
            acp,
            projection,
            identity,
            policy_id,
            resource_name,
            collection_id,
            inputs,
        )
        .await?;
        if !collection.has_access {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Transaction context that provides storage access within a transaction.
///
/// This is implemented by the database layer to provide transaction-scoped
/// document fetching and mutation. A context is one request's view of a
/// transaction on the worker that owns it ([`TxnTurn::context`](super::TxnTurn::context)):
/// the handles it gives out share the transaction for that request and are
/// dropped before the request ends.
pub trait TransactionContext: MaybeSendSync {
    /// Get the transaction ID.
    fn id(&self) -> &str;

    /// Check if this is a read-only transaction.
    fn is_readonly(&self) -> bool;

    /// Get a document fetcher scoped to this transaction.
    fn doc_fetcher(&self) -> Arc<dyn DocFetcher>;

    /// Get a document mutator scoped to this transaction.
    ///
    /// Returns `None` if this is a read-only transaction or if mutators
    /// are not supported by this context implementation.
    ///
    /// The mutator shares the same underlying transaction as the fetcher,
    /// so all read and write operations are within the same transaction context.
    fn doc_mutator(&self) -> Option<Arc<dyn DocMutator>> {
        None
    }

    /// Get a collection provider scoped to this transaction.
    ///
    /// Returns `None` by default. Implementations that support transaction-scoped
    /// schema resolution should override this to return a provider that reads
    /// from the transaction's uncommitted state (falling back to the process-wide cache).
    fn collection_provider(&self) -> Option<Arc<dyn CollectionProvider>> {
        None
    }

    /// Get transaction-scoped projection for external ACP registration writes.
    fn acp_registration_projection(&self) -> Option<Arc<AcpRegistrationProjection>> {
        None
    }

    /// Check if the transaction is still active (not yet committed or rolled back).
    ///
    /// Returns `true` if the transaction can still be used for queries.
    /// Returns `false` if the transaction has been consumed via commit/discard.
    ///
    /// # Implementation Note
    ///
    /// Concrete implementations SHOULD override this method if they track
    /// consumption state. The default returns `true`, which is appropriate
    /// for implementations that don't track state or where checking state
    /// synchronously isn't feasible (e.g., when state is behind an async mutex).
    fn is_active(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_unregistered_doc_denies_overlay_and_shared_reads() {
        workers::block_on(async {
            use acp::read_access::{check_doc_read_access, ObjectAccessChecker};
            let acp = NoopDocumentAcp(false);
            let projection = AcpRegistrationProjection::detached();
            let checker = crate::txn::OverlayChecker {
                acp: &acp,
                projection: Some(&projection),
                identity: &Identity::Anonymous,
            };
            let absent = checker
                .object_access("policy", "User", "doc-1")
                .await
                .unwrap();
            assert!(!absent.has_access);
            assert!(!absent.explicit);
            assert!(!check_doc_read_access(
                &checker,
                "policy",
                "User",
                "collection",
                false,
                "doc-1",
            )
            .await
            .unwrap());
        })
        .unwrap()
    }

    #[test]
    fn unregistered_branchable_doc_falls_back_to_collection_policy() {
        workers::block_on(async {
            let projection = AcpRegistrationProjection::detached();
            let memo = crate::prepared::PreparedInputMemo::default();
            let doc_request = crate::prepared::PreparedInputRequest::new(
                crate::prepared::PreparedInputKind::AcpDecision,
                serde_json::to_vec(&(
                    "anonymous",
                    "registered",
                    "policy",
                    "User",
                    "doc-1",
                    None::<String>,
                ))
                .unwrap(),
            );
            assert_eq!(&*memo.insert(doc_request, b"false".as_slice()), b"false");
            let request_bytes = serde_json::to_vec(&(
                "anonymous",
                "registered",
                "policy",
                "User",
                "collection-1",
                None::<String>,
            ))
            .unwrap();
            let request = crate::prepared::PreparedInputRequest::new(
                crate::prepared::PreparedInputKind::AcpDecision,
                request_bytes,
            );
            memo.insert(request, std::sync::Arc::<[u8]>::from(b"true".as_slice()));
            let permission_request = crate::prepared::PreparedInputRequest::new(
                crate::prepared::PreparedInputKind::AcpDecision,
                serde_json::to_vec(&(
                    "anonymous",
                    "read",
                    "policy",
                    "User",
                    "collection-1",
                    None::<String>,
                ))
                .unwrap(),
            );
            memo.insert(
                permission_request,
                std::sync::Arc::<[u8]>::from(b"false".as_slice()),
            );

            let allowed = check_doc_read_access_with_prepared_inputs(
                &NoopDocumentAcp(true),
                Some(&projection),
                &Identity::Anonymous,
                "policy",
                "User",
                "collection-1",
                true,
                "doc-1",
                Some(&memo),
            )
            .await
            .unwrap();
            assert!(
                !allowed,
                "a public unregistered object still falls back to its branchable collection"
            );
        })
        .unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl DocumentACP for NoopDocumentAcp {
        fn unregistered_documents_are_public(&self) -> bool {
            self.0
        }

        async fn register_doc_object(
            &self,
            _identity: &Did,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> AcpResult<()> {
            Ok(())
        }

        async fn is_doc_registered(
            &self,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> AcpResult<bool> {
            Ok(false)
        }

        async fn get_doc_owner(
            &self,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> AcpResult<Option<Did>> {
            Ok(None)
        }

        async fn check_doc_access(
            &self,
            _identity: &Identity,
            _permission: DocumentPermission,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> AcpResult<bool> {
            Ok(false)
        }

        async fn add_actor_relationship(
            &self,
            _requestor: &Did,
            _target: &Did,
            _policy_id: &str,
            _collection_id: &str,
            _doc_id: &str,
            _relation: &str,
            _managing_relations: &[String],
        ) -> AcpResult<bool> {
            Ok(false)
        }

        async fn delete_actor_relationship(
            &self,
            _requestor: &Did,
            _target: &Did,
            _policy_id: &str,
            _collection_id: &str,
            _doc_id: &str,
            _relation: &str,
            _managing_relations: &[String],
        ) -> AcpResult<bool> {
            Ok(false)
        }

        async fn unregister_doc_object(
            &self,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> AcpResult<()> {
            Ok(())
        }
    }

    struct NoopDocumentAcp(bool);
}
