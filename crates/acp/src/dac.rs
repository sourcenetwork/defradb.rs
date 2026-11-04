//! Document ACP trait (matches Go acp/dac/dac.go)
//!
//! This trait defines the interface for document-level access control.

use async_trait::async_trait;
use defra_core::RequestContext;
use identity::Did;
use storage::corekv::MaybeSendSync;
use zanzibar::types::Subject;

use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::permission::DocumentPermission;

/// Document ACP interface (matches Go acp/dac/dac.go)
///
/// This trait provides document-level access control operations:
/// - Registration: Documents are registered with an owner when created with identity
/// - Access checks: Verify if an identity has permission to perform an operation
/// - Relationship management: Add/remove actor relationships for sharing
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DocumentACP: MaybeSendSync {
    /// Whether local reads and writes must share the authorized transaction.
    fn supports_transactions(&self) -> bool {
        false
    }

    /// Bind local authorization and registration to the same transaction as its rows.
    /// Remote backends return None and remain at the network boundary.
    fn bind_transaction(
        &self,
        _view: datastore::NamespaceView,
    ) -> Option<std::sync::Arc<dyn DocumentACP>> {
        None
    }

    /// This provider serving the request `context` serves: one that acts on
    /// its callers' behalf with the credentials the request carries (see
    /// [`RequestContext::bearer_token_for`]). Providers that present no
    /// request credentials return None and serve every request as they are.
    fn bind_request(&self, _context: &RequestContext) -> Option<std::sync::Arc<dyn DocumentACP>> {
        None
    }

    /// The bearer token `did` presents to this provider: the one of the
    /// request it is bound to, or of [`RequestContext::ambient`] when it is
    /// bound to none.
    fn bearer_token_for(&self, did: &str) -> Option<String> {
        RequestContext::ambient().bearer_token_for(did)
    }

    /// Whether missing registrations allow public access or require a collection grant.
    /// Remote backends may deny absence, including retired policies and archived objects.
    fn unregistered_documents_are_public(&self) -> bool {
        true
    }

    /// Confirm a declared relationship using this provider's current snapshot.
    async fn validate_declared_relation(
        &self,
        _policy_id: &str,
        _resource: &str,
        _relation: &str,
    ) -> Result<()> {
        Err(Error::UnsupportedSubject(
            "transactional relationship validation".into(),
        ))
    }

    /// Register a document with creator as owner.
    ///
    /// This is called when a document is created with an identity.
    /// The identity becomes the owner of the document.
    ///
    /// # Arguments
    /// * `identity` - The DID of the document creator (becomes owner)
    /// * `policy_id` - The policy ID from the collection
    /// * `resource_name` - The resource name from the policy
    /// * `doc_id` - The document ID being registered
    async fn register_doc_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()>;

    /// Whether this provider reads and settles the registrations creates owe
    /// it (see [`crate::pending`]), as [`crate::ExternalDocumentAcp`] does. A
    /// non-transactional provider that does not is wrapped in one by the
    /// database it is installed in.
    fn settles_owed_registrations(&self) -> bool {
        false
    }

    /// Complete the registration a create owes this provider for `doc_id`
    /// (see [`crate::pending`]): register the recorded creator and clear the
    /// record. Nothing is owed when no record exists, which is always the case
    /// for a provider that registers inside the create's transaction.
    async fn redrive_registration(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let _ = (policy_id, resource_name, doc_id);
        Ok(())
    }

    /// Register an ACP object with creator as owner.
    ///
    /// Documents use `id = docID`; branchable collections use
    /// `id = collectionID` to gate their collection-level commit DAG.
    async fn register_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> Result<()> {
        self.register_doc_object(identity, policy_id, resource_name, object_id)
            .await
    }

    /// Check if document/object is registered with ACP.
    ///
    /// Documents created without identity may be unregistered; public access is backend-specific.
    /// Documents created with identity are registered.
    async fn is_doc_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool>;

    /// Get the current owner of a registered document, if available.
    ///
    /// This is used by replication paths that need to preserve the document's
    /// owner identity when replaying previously-created blocks.
    async fn get_doc_owner(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<Did>> {
        let _ = (policy_id, resource_name, doc_id);
        Ok(None)
    }

    /// Check if identity has permission on document.
    ///
    /// # Access Rules
    /// 1. If document is unregistered -> apply the backend's public-access rule
    /// 2. If identity is Anonymous -> deny (anonymous cannot access registered docs)
    /// 3. If identity is owner -> allow all
    /// 4. Check if identity has specific relation granting permission
    ///
    /// # Arguments
    /// * `identity` - The identity of the requester (Anonymous or Authenticated)
    /// * `permission` - The permission being requested
    /// * `policy_id` - The policy ID from the collection
    /// * `resource_name` - The resource name from the policy
    /// * `doc_id` - The document ID being accessed
    async fn check_doc_access(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool>;

    /// Create a persisted access decision for a caller/object/permission check.
    ///
    /// Vera-backed ACP can use this to mint a decision that downstream
    /// signers verify via a light client before producing a signature.
    ///
    /// Returns `Ok(Some(decision_id))` when the backend created a reusable
    /// decision, `Ok(None)` when the backend does not support this flow.
    async fn create_access_decision(
        &self,
        identity: &Identity,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        permission: &str,
    ) -> Result<Option<String>> {
        let _ = (identity, policy_id, resource_name, object_id, permission);
        Ok(None)
    }

    /// Add actor relationship (sharing).
    ///
    /// The requestor must be the document owner or have a managing relation
    /// that grants them authority to add the requested relation.
    ///
    /// # Arguments
    /// * `requestor` - The DID making the request (must be owner or manager)
    /// * `target` - The DID to grant the relation to
    /// * `policy_id` - The policy ID from the collection schema
    /// * `collection_id` - The collection ID (used as resource identifier)
    /// * `doc_id` - The document ID
    /// * `relation` - The relation to grant (e.g., "reader", "updater")
    /// * `managing_relations` - Relations that can manage `relation` (from policy)
    ///
    /// # Returns
    /// * `Ok(true)` - Relationship was added
    /// * `Ok(false)` - Relationship already exists
    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool>;

    /// Add a relationship whose target may be any [`Subject`] — an actor DID,
    /// the all-actors wildcard, a cross-object edge, or a userset — subject to
    /// the policy's declared relation `types:`.
    ///
    /// The default implementation supports only actor subjects (a DID or `*`),
    /// delegating to [`add_actor_relationship`](Self::add_actor_relationship),
    /// and rejects anything a bare-DID backend cannot represent with
    /// [`Error::UnsupportedSubject`]. Backends that model the full subject
    /// language (the Zanzibar backend) override this to store cross-object and
    /// userset edges, validating the subject against the policy first.
    async fn add_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        let actor = match target {
            Subject::Entity(did) => did,
            Subject::Wildcard => Did::wildcard(),
            other => return Err(Error::UnsupportedSubject(other.to_string())),
        };
        self.add_actor_relationship(
            requestor,
            &actor,
            policy_id,
            collection_id,
            doc_id,
            relation,
            managing_relations,
        )
        .await
    }

    /// Remove actor relationship.
    ///
    /// The requestor must be the document owner or have a managing relation
    /// that grants them authority to remove the requested relation.
    ///
    /// # Arguments
    /// * `requestor` - The DID making the request (must be owner or manager)
    /// * `target` - The DID to remove the relation from
    /// * `policy_id` - The policy ID from the collection schema
    /// * `collection_id` - The collection ID
    /// * `doc_id` - The document ID
    /// * `relation` - The relation to remove
    /// * `managing_relations` - Relations that can manage `relation` (from policy)
    ///
    /// # Returns
    /// * `Ok(true)` - Relationship was removed
    /// * `Ok(false)` - Relationship didn't exist
    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool>;

    /// Remove a relationship whose target may be any [`Subject`] — an actor DID,
    /// the all-actors wildcard, a cross-object edge, or a userset.
    ///
    /// This is the revoke mirror of [`add_relationship`](Self::add_relationship):
    /// an edge that can be granted through a structured subject must also be
    /// removable through one, so a cross-object edge can be revoked rather than
    /// leaking access forever.
    ///
    /// The default implementation supports only actor subjects (a DID or `*`),
    /// delegating to [`delete_actor_relationship`](Self::delete_actor_relationship),
    /// and rejects anything a bare-DID backend cannot represent with
    /// [`Error::UnsupportedSubject`]. Backends that model the full subject
    /// language (the Zanzibar backend) override this to remove cross-object and
    /// userset edges.
    async fn delete_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        let actor = match target {
            Subject::Entity(did) => did,
            Subject::Wildcard => Did::wildcard(),
            other => return Err(Error::UnsupportedSubject(other.to_string())),
        };
        self.delete_actor_relationship(
            requestor,
            &actor,
            policy_id,
            collection_id,
            doc_id,
            relation,
            managing_relations,
        )
        .await
    }

    /// Remove provider-local registration state when a document is deleted.
    ///
    /// Providers backed by shared authorization state may keep the object
    /// registered after a local document tombstone: a replica's delete can be
    /// rejected elsewhere, and removing shared grants early would affect
    /// documents that remain live there. Global object archival is a separate
    /// lifecycle operation for those providers.
    ///
    /// # Arguments
    /// * `policy_id` - The policy ID from the collection
    /// * `resource_name` - The resource name from the policy
    /// * `doc_id` - The document ID being unregistered
    async fn unregister_doc_object(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()>;
}

/// `acp` serving the request `context` serves (see
/// [`DocumentACP::bind_request`]).
pub fn bound_to_request(
    acp: &std::sync::Arc<dyn DocumentACP>,
    context: &RequestContext,
) -> std::sync::Arc<dyn DocumentACP> {
    acp.bind_request(context)
        .unwrap_or_else(|| std::sync::Arc::clone(acp))
}

/// Runs `call` against `acp` for `did` presenting `token`, outside the
/// request that captured them (a hook a commit runs later): `acp` bound to a
/// context carrying that bearer, which [`RequestContext::mirror`] also
/// installs for test doubles that read it the old way.
pub async fn with_request_bearer<T, F, Fut>(
    acp: &std::sync::Arc<dyn DocumentACP>,
    did: Option<String>,
    token: Option<String>,
    call: F,
) -> T
where
    F: FnOnce(std::sync::Arc<dyn DocumentACP>) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let context = RequestContext::ambient().with_bearer(did, token);
    context.mirror(call(bound_to_request(acp, &context))).await
}
