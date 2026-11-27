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
use crate::object_registration::{AcpObject, RegistrationFailure};
use crate::permission::DocumentPermission;
use crate::DerivedRelationship;

/// Told when this node changes an object's relationships on its provider
/// ([`DocumentACP::watch_object_changes`]). Called after the change, on the
/// task that made it, which it must not hold up.
pub trait ObjectChangeListener: MaybeSendSync {
    fn changed(&self, policy_id: &str, resource_name: &str, object_id: &str);
}

/// Told when the provider refuses an owed relationship change for good
/// ([`DocumentACP::watch_owed_refusals`]). `kept` says whether the change
/// stays parked as refused (its actor was checked at staging) or was
/// removed. Called after the record is written, on the task that wrote it,
/// which it must not hold up.
pub trait OwedRefusalListener: MaybeSendSync {
    fn refused(&self, change: &crate::outbox::OwedChange, reason: &str, kept: bool);
}

/// A caller and expiration established by a credential's verification owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCaller {
    pub did: identity::Did,
    pub expires_at: u64,
}

/// One relationship stored on a document: its relation and its subject, as
/// a DID, `*`, or `resource:object#relation`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocumentRelationship {
    pub relation: String,
    pub subject: String,
}

#[derive(Clone, Copy)]
pub struct ActorRelationshipObservation<'a> {
    pub requester: &'a Did,
    pub target: &'a Did,
    pub policy: &'a str,
    pub resource: &'a str,
    pub document: &'a str,
    pub relation: &'a str,
}

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
    /// Manage a document relation as the actual native NAC administrator.
    /// Implementations must evaluate active NAC and perform DAC writes through
    /// this same view. This is unavailable for unbound or remote providers.
    async fn change_node_admin_relationship(
        &self,
        _view: datastore::NamespaceView,
        _actor: &Did,
        _target: Subject,
        _policy_id: &str,
        _resource: &str,
        _doc_id: &str,
        _relation: &str,
        _delete: bool,
    ) -> Result<bool> {
        Err(Error::UnsupportedAdministrativeRelationship)
    }

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

    /// Bind local authorization to a transaction snapshot for a read that
    /// decides nothing the transaction writes. Such reads never validate.
    fn bind_read_transaction(
        &self,
        view: datastore::NamespaceView,
    ) -> Option<std::sync::Arc<dyn DocumentACP>> {
        self.bind_transaction(view)
    }

    /// Whether this provider has a transaction stage the relationship
    /// changes it makes as owed ([`transaction_outbox`](Self::transaction_outbox)
    /// gives a place for them), rather than applying them in it or not at all.
    fn stages_relationship_changes(&self) -> bool {
        false
    }

    /// Where a transaction stages the relationship changes it owes this
    /// provider, for a provider that cannot join a transaction
    /// ([`bind_transaction`](Self::bind_transaction) is `None`) but keeps
    /// what is owed to it (see [`crate::outbox`]). `view` is the transaction's
    /// ACP namespace and `staged` its count of staged changes. `None` for a
    /// provider that joins the transaction, and for one that keeps nothing.
    fn transaction_outbox(
        &self,
        _view: &datastore::NamespaceView,
        _staged: crate::outbox::StagedCount,
    ) -> Option<crate::outbox::TransactionOutbox> {
        None
    }

    fn outbox_authority_provider(&self) -> Option<std::sync::Arc<dyn DocumentACP>> {
        None
    }

    /// `did` in the one spelling this provider keeps an actor under. A key
    /// that can be written in several ways names one actor, and what is
    /// stored about it is compared under this spelling.
    fn canonical_actor(&self, did: &Did) -> Did {
        did.clone()
    }

    /// The DID this provider signs its own commands as, when it has one. A
    /// node registers a replicated document as this actor
    /// ([`crate::replica`]), and counts it, beside its identity, as itself
    /// when it asks whether it can read a replicated document.
    fn own_actor(&self) -> Option<Did> {
        None
    }

    /// Tell `listener` whenever this node changes an object's relationships
    /// on the provider: a relationship set or deleted through it, an owed
    /// change or registration it settled. A provider that keeps no such
    /// record tells nothing. The first listener stays.
    fn watch_object_changes(&self, _listener: std::sync::Arc<dyn ObjectChangeListener>) {}

    /// What this node still owes, or waits for, on a replicated document,
    /// from committed state alone: the merge gate's input
    /// ([`crate::ReplicaOwed`]). Nothing for a provider that keeps no outbox.
    async fn replica_owed(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> Result<crate::ReplicaOwed> {
        Ok(crate::ReplicaOwed::default())
    }

    /// Record, in the transaction whose ACP namespace `view` is, the unit a
    /// receiving node owes for a replicated document it registers as itself:
    /// the registration and the embedder's grants for its creator. Nothing
    /// is recorded when a registration of the document is already owed.
    async fn stage_replica_unit(
        &self,
        _view: &datastore::NamespaceView,
        unit: &crate::ReplicaUnit<'_>,
    ) -> Result<()> {
        Err(Error::ProviderUnavailable(format!(
            "this provider keeps no outbox to register replicated document {}",
            unit.doc_id
        )))
    }

    /// Record, in the transaction whose ACP namespace `view` is, that the
    /// owner of a replicated document owes `creator`'s grants and this node
    /// waits for them.
    async fn adopt_replica(
        &self,
        _view: &datastore::NamespaceView,
        _policy_id: &str,
        _resource_name: &str,
        doc_id: &str,
        _creator: &Did,
    ) -> Result<()> {
        Err(Error::ProviderUnavailable(format!(
            "this provider keeps no outbox to adopt replicated document {doc_id}"
        )))
    }

    /// Submit what this node owes on a replicated document, and drop its
    /// marker once nothing is owed or waited for any more. Nothing for a
    /// provider that keeps no outbox.
    async fn settle_replica(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> Result<()> {
        Ok(())
    }

    /// Whether this provider can be sent a relationship change with this
    /// kind of subject. A change it cannot take is not staged. The default
    /// is what the default relationship calls take: one actor, or all.
    fn submits_subject(&self, subject: &Subject) -> bool {
        matches!(subject, Subject::Entity(_) | Subject::Wildcard)
    }

    /// Whether deleting a document archives its object on this provider as
    /// an owed change staged in the delete's transaction
    /// ([`crate::TransactionOutbox::stage_archive`]), made as the object's
    /// owner whoever deletes; the delete is refused before it commits when
    /// the archive cannot be staged. The default archives nothing that way.
    fn stages_archive_on_delete(&self) -> bool {
        false
    }

    /// Archive one object as `actor`: the provider removes every
    /// relationship on it and its owner no longer counts. Sent for an owed
    /// archive ([`crate::OwedOp::Archive`]). The default takes none.
    async fn archive_doc_object(
        &self,
        actor: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let _ = (actor, policy_id, resource_name, doc_id);
        Err(Error::UnsupportedArchive)
    }

    /// What lets the node submit a relationship change as `actor` after the
    /// request that made it is gone: the node's own key for `actor`, or the
    /// delegation the request this provider serves carries for it. With
    /// neither, [`Error::OwedAuthorityUnavailable`]. A provider that takes
    /// no credentials submits as any DID.
    fn owed_authority(&self, _actor: &Did) -> Result<crate::outbox::OwedAuthority> {
        Ok(crate::outbox::OwedAuthority::NodeKey)
    }

    /// The journal this provider sends owed changes through, when its
    /// worker keeps each request until a verified receipt lets it go. With
    /// one, a change is marked in flight before it is sent and decided by
    /// the journal, a receipt or the provider's note after a crash;
    /// without, each change goes through
    /// [`submit_owed_change`](Self::submit_owed_change).
    fn owed_journal(&self) -> Option<std::sync::Arc<dyn crate::outbox::OwedJournal>> {
        None
    }

    /// Tell `listener` of every owed change the provider refuses for good.
    /// A provider that keeps no outbox has none to tell of.
    fn watch_owed_refusals(&self, _listener: std::sync::Arc<dyn OwedRefusalListener>) {}

    /// Submit one owed relationship change as its acting DID, with the
    /// authority recorded with it and none of the request this provider may
    /// be bound to. Submitting a change the provider already holds is
    /// [`Submission::Applied`](crate::outbox::Submission::Applied) wherever
    /// the provider can tell.
    ///
    /// The caller may stop waiting for the answer, and submits the later
    /// changes of the tuple afterwards. A submission must therefore never
    /// take effect after one made later through this provider: what is
    /// still in flight when the next is made has to execute before it or
    /// not at all. vera.rs holds one journaled request per worker and
    /// prepares the next only once that one has a verified receipt.
    async fn submit_owed_change(
        &self,
        change: &crate::outbox::OwedChange,
    ) -> crate::outbox::Submission {
        crate::outbox::submit_relationship(self, change).await
    }

    /// Refuse to serve when the store holds what is owed to this provider in
    /// a layout this version cannot read ([`Error::LegacyOwedChanges`]). A
    /// node asks once at start. Nothing to refuse for a provider that keeps
    /// no outbox.
    async fn check_owed_layout(&self) -> Result<()> {
        Ok(())
    }

    /// Submit the relationship changes owed to this provider that `scope`
    /// names, once, in order. Nothing is owed to a provider that keeps no
    /// outbox.
    async fn settle_owed_changes(&self, scope: crate::OwedScope) -> Result<crate::OwedSettlement> {
        let _ = scope;
        Ok(crate::OwedSettlement::default())
    }

    /// What is owed to this provider that `filter` names: the registrations
    /// creates owe it and the relationship changes transactions staged, each
    /// with its state and the reason of its last failed submission. The
    /// credential recorded with an entry is never part of it. Nothing is
    /// owed to a provider that keeps no outbox.
    ///
    /// This reads for whoever calls it: the caller decides who may see
    /// which actor's entries.
    async fn owed_entries(
        &self,
        filter: crate::outbox::OwedFilter<'_>,
    ) -> Result<Vec<crate::outbox::OwedEntry>> {
        let _ = filter;
        Ok(Vec::new())
    }

    /// Wait until what the explicit transaction `txn` of this run owes
    /// this provider is settled or has come to rest, and say which: for the
    /// entries of `only`, or for all of them. What can still be sent is
    /// submitted once first. The notes of discarded changes the answer
    /// holds are deleted: whoever is given it has been told. The wait has
    /// no end of its own; its caller bounds it.
    async fn await_owed(&self, txn: u64, only: Option<&Did>) -> Result<crate::outbox::OwedWait> {
        let _ = (txn, only);
        Ok(crate::outbox::OwedWait::Settled)
    }

    /// A record of something owed to this provider was written or deleted
    /// by a transaction that has now committed: whoever waits for what is
    /// owed looks again.
    fn owed_changed(&self) {}

    /// Take the credential of the request this provider serves
    /// ([`bind_request`](Self::bind_request)) as the fresh authority of one
    /// entry `actor` owes, and submit that entry. The credential is asked
    /// about the owed command by [`command_authority`](Self::command_authority),
    /// as at admission; one it refuses records nothing. An entry that is
    /// not owed, or is owed by another actor, is
    /// [`Error::OwedEntryNotFound`].
    async fn reauthorize_owed(
        &self,
        actor: &Did,
        entry: &crate::outbox::OwedRef,
    ) -> Result<crate::outbox::OwedOutcome> {
        Err(Error::OwedEntryNotFound {
            entry: entry.to_string(),
            actor: actor.to_string(),
        })
    }

    /// Delete the record of the refused relationship change at `seq`, which
    /// `actor` staged, without this provider holding the change. A change
    /// in any other state is [`Error::OwedChangeNotRefused`].
    async fn abandon_owed_change(&self, actor: &Did, seq: u64) -> Result<()> {
        Err(Error::OwedEntryNotFound {
            entry: crate::outbox::OwedRef::Change { seq }.to_string(),
            actor: actor.to_string(),
        })
    }

    /// This provider serving the request `context` serves: one that acts on
    /// its callers' behalf with the credentials the request carries (see
    /// [`RequestContext::bearer_token_for`]). Providers that present no
    /// request credentials return None and serve every request as they are.
    fn bind_request(&self, _context: &RequestContext) -> Option<std::sync::Arc<dyn DocumentACP>> {
        None
    }

    /// The DID that signed `token`, when `token` is a delegation this ACP may
    /// act under now for that DID through this node. `None` when `token` is
    /// not a delegation, or this ACP takes none: the caller is then
    /// authenticated by its identity token.
    fn caller_of_delegation(
        &self,
        _token: &str,
    ) -> Option<std::result::Result<AuthenticatedCaller, String>> {
        None
    }

    /// The bearer token `did` presents to this provider: the one the
    /// request it is bound to carries for `did`, and none when it is bound
    /// to no request.
    fn bearer_token_for(&self, did: &str) -> Option<String> {
        RequestContext::ambient().bearer_token_for(did)
    }

    /// What lets the node send one command as `actor`: what
    /// [`owed_authority`](Self::owed_authority) gives for any command, or a
    /// delegation the request this provider serves carries for `actor` that
    /// is bound to exactly that command. A delegation bound to another
    /// command is [`Error::OwedAuthorityUnavailable`], as no authority at
    /// all is.
    fn command_authority(
        &self,
        actor: &Did,
        _command: crate::ProviderCommand<'_>,
    ) -> Result<crate::outbox::OwedAuthority> {
        self.owed_authority(actor)
    }

    /// Whether missing registrations allow public access or require a collection grant.
    /// Remote backends may deny absence, including retired policies and archived objects.
    fn unregistered_documents_are_public(&self) -> bool {
        true
    }

    /// Whether every access decision this provider gives is proven against
    /// its finalized state at a revision within a freshness limit, and fails
    /// when no such revision can be read: no decision comes from a cache
    /// that outlives that limit, and none from an unproven answer. A
    /// verified update or delete on such a provider needs no credential
    /// beside the permission it decides (see
    /// `query::runner::remote_authority`).
    fn proves_decisions(&self) -> bool {
        false
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
    /// * `relationships` - What the document carries through its
    ///   `@policy(relation:)` fields ([`DerivedRelationship`]): an external
    ///   provider checks the collection's `create` over them and stores them
    ///   with the owner; the local engine ignores them
    async fn register_doc_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        relationships: &[DerivedRelationship],
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

    /// [`Self::redrive_registration`] for each of `objects`, in their order:
    /// what each came to. A provider that settles owed registrations sends
    /// those one owner owes together ([`Self::register_objects`]); every
    /// object is answered for, whatever became of the others.
    async fn redrive_registrations(&self, objects: &[AcpObject<'_>]) -> Vec<Result<()>> {
        let mut outcomes = Vec::with_capacity(objects.len());
        for object in objects {
            outcomes.push(
                self.redrive_registration(object.policy_id, object.resource, object.object_id)
                    .await,
            );
        }
        outcomes
    }

    /// Register each of `objects` with `identity` as its owner, in order:
    /// what [`Self::register_object`] does for each, which a provider that
    /// can sends as one list it applies all or none. It stops at the first
    /// that fails, and says which ([`RegistrationFailure`]).
    async fn register_objects(
        &self,
        identity: &Did,
        objects: &[AcpObject<'_>],
    ) -> std::result::Result<(), RegistrationFailure> {
        for (index, object) in objects.iter().enumerate() {
            let registered = if object.relationships.is_empty() {
                self.register_object(
                    identity,
                    object.policy_id,
                    object.resource,
                    object.object_id,
                )
                .await
            } else {
                self.register_doc_object(
                    identity,
                    object.policy_id,
                    object.resource,
                    object.object_id,
                    object.relationships,
                )
                .await
            };
            registered.map_err(|error| RegistrationFailure::new(index, error))?;
        }
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
        self.register_doc_object(identity, policy_id, resource_name, object_id, &[])
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

    /// Verify that this receiver belongs to the named owner's group and check
    /// the signer's membership at the same finalized state. Only a proven
    /// nonmember returns false.
    async fn replica_group_member(
        &self,
        _group: &crate::ReplicaGroup,
        _signer: &Did,
    ) -> Result<bool> {
        Err(Error::UnsupportedReplicaMembership)
    }

    /// Evaluate a named permission for an explicit actor. Backends that cannot
    /// evaluate named permissions must fail rather than infer a grant.
    async fn check_named_permission(
        &self,
        _subject: &Did,
        _permission: &str,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> Result<bool> {
        Err(Error::UnsupportedNamedPermission)
    }

    /// Every relationship stored on one document, for its owner or for a
    /// requester holding a relation that manages some relation on that
    /// resource. Refusal is [`Error::NotManager`]; backends that cannot list
    /// fail with [`Error::UnsupportedRelationshipListing`].
    async fn list_relationships(
        &self,
        _requestor: &Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> Result<Vec<DocumentRelationship>> {
        Err(Error::UnsupportedRelationshipListing)
    }

    async fn observe_actor_relationship(
        &self,
        _observation: ActorRelationshipObservation<'_>,
    ) -> Result<bool> {
        Err(Error::UnsupportedRelationshipListing)
    }

    async fn verify_transport_sentinel(
        &self,
        _query: &crate::TransportSentinelQuery,
    ) -> Result<bool> {
        Err(Error::UnsupportedRelationshipListing)
    }

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

    /// Remove provider-local registration state for a document: its owner
    /// and every relationship on it.
    ///
    /// A delete calls this only on a provider that reads a missing object as
    /// denied ([`Self::unregistered_documents_are_public`] false): elsewhere
    /// the object keeps deciding who reads the history the node retains, and
    /// is removed only when a purge removes that history.
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
