use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use acp::{DocumentPermission, Identity};
use async_trait::async_trait;
use defra_core::browser_sync::{
    BrowserSyncDocument, BrowserSyncPull, BrowserSyncRefusal, BrowserSyncRequest,
    BrowserSyncResponse, DEFAULT_SYNC_PAGE_SIZE, MAX_SYNC_BODY_BYTES, MAX_SYNC_ID_BYTES,
    MAX_SYNC_PAGE_SIZE, MAX_SYNC_PAYLOAD_BYTES, MAX_SYNC_RELATIONSHIPS_PER_DOCUMENT,
};
use defra_http::router::{BrowserSyncError, BrowserSyncOperations, BrowserSyncResult};
use storage::corekv::Store;

pub struct BrowserSyncAdapter<S: Store + 'static> {
    engine: db::merge::BrowserSyncEngine<S>,
    document_acp: Arc<dyn acp::DocumentACP>,
}

struct PendingSyncDocument {
    document: db::merge::ValidatedBrowserSyncDocument,
    collection: db::Collection,
    register_owner: Option<identity::Did>,
    relationships: Vec<PendingRelationship>,
}

/// A grant from the push, resolved to the two subjects
/// `add_actor_relationship` can represent.
struct PendingRelationship {
    relation: String,
    target: identity::Did,
}

impl<S: Store + 'static> BrowserSyncAdapter<S> {
    pub fn new_arc(
        database: Arc<db::DB<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
        txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>>,
    ) -> Arc<dyn BrowserSyncOperations> {
        Arc::new(Self {
            engine: match txn_broadcaster {
                Some(broadcaster) => {
                    db::merge::BrowserSyncEngine::with_broadcaster(database, broadcaster)
                }
                None => db::merge::BrowserSyncEngine::new(database),
            },
            document_acp,
        })
    }

    fn collection(&self, collection_id: &str) -> BrowserSyncResult<db::Collection> {
        self.engine
            .database()
            .find_collection_by_id(collection_id)
            .map_err(|error| BrowserSyncError::Internal(error.to_string()))?
            .ok_or_else(|| {
                BrowserSyncError::InvalidInput(format!(
                    "collection '{collection_id}' is not registered"
                ))
            })
    }

    async fn can_access(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        collection: &db::Collection,
        doc_id: &str,
        bypass_dac: bool,
    ) -> BrowserSyncResult<bool> {
        if bypass_dac {
            return Ok(true);
        }
        db::collection::acp::check_doc_permission(
            self.document_acp.as_ref(),
            identity,
            permission,
            collection.schema(),
            doc_id,
            self.engine.database().node_did().as_ref(),
        )
        .await
        .map_err(|error| BrowserSyncError::Internal(error.to_string()))
    }

    async fn prepare_document(
        &self,
        document: &BrowserSyncDocument,
        identity: &Identity,
        bypass_dac: bool,
    ) -> BrowserSyncResult<PendingSyncDocument> {
        let source = document;
        let document = self
            .engine
            .validate_document(document)
            .map_err(map_engine_error)?;
        let collection = self.collection(document.collection_id())?;
        // The delta before the permission, because what changes nothing is not
        // an update. A peer that offers back a document it was given holds no
        // block this node lacks, and refusing that costs the exchange it rode
        // in on — the pull included. A payload that does carry a block is an
        // update and is checked as one, so this narrows what needs permission
        // without widening what may be written.
        if !self
            .engine
            .holds_every_block(&document)
            .await
            .map_err(map_engine_error)?
            && !self
                .can_access(
                    identity,
                    DocumentPermission::Update,
                    &collection,
                    document.doc_id(),
                    bypass_dac,
                )
                .await?
        {
            return Err(BrowserSyncError::Forbidden(format!(
                "update access denied for document {}",
                document.doc_id()
            )));
        }

        let was_registered = match collection.schema().policy.as_ref() {
            Some(policy) => self
                .document_acp
                .is_doc_registered(&policy.id, &policy.resource_name, document.doc_id())
                .await
                .map_err(|error| BrowserSyncError::Internal(error.to_string()))?,
            None => false,
        };
        let existed = self
            .engine
            .document_ref(document.doc_id())
            .await
            .map_err(map_engine_error)?
            .is_some();
        // Ownership follows the replication convention (acp_merge_handler):
        // only the creator cryptographically verified from the genesis block's
        // signature may be registered — never the transport caller, who might
        // be pushing someone else's DAG. An unsigned genesis has no verifiable
        // author and stays unregistered (unregistered == public under Local
        // ACP, matching Go's replication semantics).
        let register_owner = if !existed && !was_registered {
            document
                .verified_genesis_creator()
                .map(|creator| {
                    identity::Did::try_from(creator.to_string()).map_err(|error| {
                        BrowserSyncError::Internal(format!(
                            "verified genesis creator DID is invalid: {error}"
                        ))
                    })
                })
                .transpose()?
        } else {
            None
        };
        let relationships = prepare_relationships(source, &collection, identity)?;
        Ok(PendingSyncDocument {
            document,
            collection,
            register_owner,
            relationships,
        })
    }

    async fn apply_document(
        &self,
        document: PendingSyncDocument,
        identity: &Identity,
    ) -> BrowserSyncResult<()> {
        let doc_id = document.document.doc_id().to_string();
        let creator = identity.did().map_or("browser-sync", |did| did.as_str());
        let policy = document.collection.schema().policy.clone();

        // Whether ACP knew this document before this request, which decides how
        // much of ACP is this request's to undo.
        let registered_before = match (policy.as_ref(), document.register_owner.as_ref()) {
            (Some(policy), Some(_)) => self
                .document_acp
                .is_doc_registered(&policy.id, &policy.resource_name, &doc_id)
                .await
                .map_err(|error| BrowserSyncError::Internal(error.to_string()))?,
            _ => true,
        };
        if let Some(owner) = document.register_owner.as_ref() {
            db::collection::acp::register_doc_if_needed(
                self.document_acp.as_ref(),
                Some(owner),
                document.collection.schema(),
                &doc_id,
            )
            .await
            .map_err(|error| BrowserSyncError::Internal(error.to_string()))?;
        }
        // Before the merge, so the document carries its grants by the time the
        // merge announces it to peers.
        //
        // A grant refused partway leaves the earlier ones durable, exactly as a
        // refused merge would, so it reverts through the same path.
        let mut granted = Vec::new();
        if let Err(error) = self
            .apply_relationships(&document, &doc_id, identity, &mut granted)
            .await
        {
            self.revert_acp(
                policy.as_ref(),
                &doc_id,
                registered_before,
                &granted,
                identity,
            )
            .await;
            return Err(error);
        }

        match self
            .engine
            .apply_validated_document(document.document, creator)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                self.revert_acp(
                    policy.as_ref(),
                    &doc_id,
                    registered_before,
                    &granted,
                    identity,
                )
                .await;
                Err(map_engine_error(error))
            }
        }
    }

    /// Put ACP back as this request found it, for a merge that did not happen.
    ///
    /// The grants precede the merge so that nothing is announced before it can
    /// be read, which leaves them durable when the merge is refused. There is
    /// no transaction to span both: the ACP backends are separate stores, one
    /// of them a chain.
    ///
    /// A document this request registered can be undone in one call —
    /// `unregister_doc_object` takes the owner tuple and every grant with it.
    /// A document that existed before keeps whatever it had, so only the grants
    /// this request created come off, and a grant that was already there stays.
    ///
    /// A failure here cannot be reported: the merge error is what the caller
    /// needs. It is logged, and re-pushing the document applies the same grants
    /// again.
    async fn revert_acp(
        &self,
        policy: Option<&schema::PolicyDescription>,
        doc_id: &str,
        registered_before: bool,
        granted: &[PendingRelationship],
        identity: &Identity,
    ) {
        let Some(policy) = policy else { return };
        if !registered_before {
            if let Err(error) = self
                .document_acp
                .unregister_doc_object(&policy.id, &policy.resource_name, doc_id)
                .await
            {
                tracing::warn!(
                    %doc_id,
                    %error,
                    "browser sync could not unregister a document whose merge was refused"
                );
            }
            return;
        }
        let Some(requestor) = identity.did() else {
            return;
        };
        for relationship in granted {
            if let Err(error) = self
                .document_acp
                .delete_actor_relationship(
                    requestor,
                    &relationship.target,
                    &policy.id,
                    &policy.resource_name,
                    doc_id,
                    &relationship.relation,
                    &[],
                )
                .await
            {
                tracing::warn!(
                    %doc_id,
                    relation = %relationship.relation,
                    %error,
                    "browser sync could not revoke a grant whose merge was refused"
                );
            }
        }
    }

    /// Apply the push's grants as the authenticated caller, never as the owner
    /// the push registered: `add_actor_relationship` then allows exactly what a
    /// second call to the relationship endpoint would, and a relay pushing
    /// somebody else's DAG still gets `NotOwner`.
    ///
    /// `managing_relations` is empty, so a manager who is not the owner has to
    /// use that endpoint, which resolves the policy's managers.
    ///
    /// `granted` collects what was actually created, and keeps it when this
    /// returns an error: a list refused partway has already applied its earlier
    /// entries, and the caller has to take those back.
    async fn apply_relationships(
        &self,
        document: &PendingSyncDocument,
        doc_id: &str,
        identity: &Identity,
        granted: &mut Vec<PendingRelationship>,
    ) -> BrowserSyncResult<()> {
        if document.relationships.is_empty() {
            return Ok(());
        }
        // Both established by `prepare_relationships` before anything in this
        // request was applied.
        let (Some(requestor), Some(policy)) =
            (identity.did(), document.collection.schema().policy.as_ref())
        else {
            return Err(BrowserSyncError::Internal(
                "sync relationships reached apply without a caller or a policy".into(),
            ));
        };
        for relationship in &document.relationships {
            let added = self
                .document_acp
                .add_actor_relationship(
                    requestor,
                    &relationship.target,
                    &policy.id,
                    &policy.resource_name,
                    doc_id,
                    &relationship.relation,
                    &[],
                )
                .await
                .map_err(|error| match error {
                    acp::Error::NotOwner { .. } | acp::Error::NotManager { .. } => {
                        BrowserSyncError::Forbidden(format!(
                            "cannot grant '{}' on document {doc_id}: {error}",
                            relationship.relation
                        ))
                    }
                    error => BrowserSyncError::Internal(error.to_string()),
                })?;
            // Only what this request added is this request's to take back.
            if added {
                granted.push(PendingRelationship {
                    relation: relationship.relation.clone(),
                    target: relationship.target.clone(),
                });
            }
        }
        Ok(())
    }

    async fn pull_documents(
        &self,
        pull: BrowserSyncPull,
        identity: &Identity,
        known_roots: &HashMap<String, Vec<String>>,
        bypass_dac: bool,
    ) -> BrowserSyncResult<BrowserSyncResponse> {
        let limit = usize::from(pull.limit.unwrap_or(DEFAULT_SYNC_PAGE_SIZE as u16));
        if limit == 0 || limit > MAX_SYNC_PAGE_SIZE {
            return Err(BrowserSyncError::InvalidInput(format!(
                "sync page size must be between 1 and {MAX_SYNC_PAGE_SIZE}"
            )));
        }
        validate_optional_id("cursor", pull.cursor.as_deref())?;
        for doc_id in &pull.doc_ids {
            validate_id("document ID", doc_id)?;
        }

        let mut refs = if pull.doc_ids.is_empty() {
            self.engine
                .document_refs()
                .await
                .map_err(map_engine_error)?
        } else {
            let mut refs = Vec::with_capacity(pull.doc_ids.len());
            for doc_id in &pull.doc_ids {
                if let Some(document_ref) = self
                    .engine
                    .document_ref(doc_id)
                    .await
                    .map_err(map_engine_error)?
                {
                    refs.push(document_ref);
                }
            }
            refs.sort_by(|left, right| left.doc_id.cmp(&right.doc_id));
            refs.dedup_by(|left, right| left.doc_id == right.doc_id);
            refs
        };
        if let Some(cursor) = pull.cursor.as_deref() {
            refs.retain(|document_ref| document_ref.doc_id.as_str() > cursor);
        }

        let mut documents = Vec::new();
        let mut payload_bytes = 0usize;
        let mut wire_bytes = serde_json::to_vec(&BrowserSyncResponse::default())
            .map_err(|error| BrowserSyncError::Internal(error.to_string()))?
            .len();
        let mut resume_cursor = pull.cursor;
        let mut has_more = false;
        for document_ref in refs {
            if documents.len() == limit {
                has_more = true;
                break;
            }

            let collection = self.collection(&document_ref.collection_id)?;
            if !self
                .can_access(
                    identity,
                    DocumentPermission::Read,
                    &collection,
                    &document_ref.doc_id,
                    bypass_dac,
                )
                .await?
            {
                continue;
            }

            let loaded = match self.engine.load_document(&document_ref).await {
                Ok(loaded) => loaded,
                // A document that cannot be represented as a sync payload can
                // never be pulled, whether it exceeds the size limit or the
                // block and root counts. Both are permanent properties of the
                // stored DAG. Failing the page would wedge this cursor
                // position — every retry re-reads the same document — so skip
                // past it and keep the page moving. Storage errors are not
                // included: those are transient and must still fail the page.
                Err(error @ db::merge::browser_sync::BrowserSyncError::TooLarge(_)) => {
                    tracing::warn!(
                        doc_id = %document_ref.doc_id,
                        collection_id = %document_ref.collection_id,
                        %error,
                        "browser sync skipped a document that cannot be represented as a sync payload"
                    );
                    resume_cursor = Some(document_ref.doc_id);
                    continue;
                }
                Err(error) => return Err(map_engine_error(error)),
            };
            let Some(document) = loaded else {
                resume_cursor = Some(document_ref.doc_id);
                continue;
            };
            if known_roots
                .get(&document.doc_id)
                .is_some_and(|roots| same_roots(roots, &document.roots))
            {
                resume_cursor = Some(document_ref.doc_id);
                continue;
            }

            let document_bytes = document_payload_bytes(&document);
            let document_wire_bytes = serde_json::to_vec(&document)
                .map_err(|error| BrowserSyncError::Internal(error.to_string()))?
                .len();
            let exceeds_limit = payload_bytes.saturating_add(document_bytes)
                > MAX_SYNC_PAYLOAD_BYTES
                || wire_bytes
                    .saturating_add(document_wire_bytes)
                    .saturating_add(MAX_SYNC_ID_BYTES + 64)
                    > MAX_SYNC_BODY_BYTES;
            if !documents.is_empty() && exceeds_limit {
                has_more = true;
                break;
            }
            // Alone on the page and still over the response limit, so no page
            // can ever carry it. Skip it for the same reason as an unloadable
            // document: erroring here would wedge this cursor position.
            if exceeds_limit {
                tracing::warn!(
                    doc_id = %document.doc_id,
                    collection_id = %document_ref.collection_id,
                    "browser sync skipped a document that exceeds the sync response limit"
                );
                resume_cursor = Some(document_ref.doc_id);
                continue;
            }
            payload_bytes = payload_bytes.saturating_add(document_bytes);
            wire_bytes = wire_bytes.saturating_add(document_wire_bytes + 1);
            resume_cursor = Some(document_ref.doc_id);
            documents.push(document);
        }

        Ok(BrowserSyncResponse {
            documents,
            next_cursor: has_more.then_some(resume_cursor).flatten(),
            refused: Vec::new(),
        })
    }
}

#[async_trait]
impl<S: Store + 'static> BrowserSyncOperations for BrowserSyncAdapter<S> {
    async fn sync(
        &self,
        request: BrowserSyncRequest,
        caller_did: Option<&str>,
        bypass_dac: bool,
    ) -> BrowserSyncResult<BrowserSyncResponse> {
        let did = caller_did
            .map(|did| {
                identity::Did::try_from(did.to_string()).map_err(|error| {
                    BrowserSyncError::Internal(format!("verified caller DID is invalid: {error}"))
                })
            })
            .transpose()?;
        let identity = Identity::from(did);
        let mut seen_doc_ids = HashSet::with_capacity(request.documents.len());
        for document in &request.documents {
            if !seen_doc_ids.insert(document.doc_id.as_str()) {
                return Err(BrowserSyncError::InvalidInput(format!(
                    "duplicate sync document {}",
                    document.doc_id
                )));
            }
        }
        let known_roots = request
            .documents
            .iter()
            .map(|document| (document.doc_id.clone(), document.roots.clone()))
            .collect();

        // A refusal is scoped to its document; every other failure is a fact
        // about the request and still fails all of it. That is what keeps the
        // batch atomic where atomicity means something — an invalid or
        // duplicated document is rejected before anything is written — while
        // one document nobody may write no longer costs the other documents,
        // the pull, and the session that was driving them.
        let mut refused = Vec::new();
        let mut pending = Vec::with_capacity(request.documents.len());
        for document in &request.documents {
            match self.prepare_document(document, &identity, bypass_dac).await {
                Ok(prepared) => pending.push(prepared),
                Err(BrowserSyncError::Forbidden(reason)) => {
                    refused.push(refusal(&document.doc_id, reason))
                }
                Err(error) => return Err(error),
            }
        }
        for document in pending {
            let doc_id = document.document.doc_id().to_string();
            match self.apply_document(document, &identity).await {
                Ok(()) => {}
                Err(BrowserSyncError::Forbidden(reason)) => refused.push(refusal(&doc_id, reason)),
                Err(error) => return Err(error),
            }
        }

        let mut response = match request.pull {
            Some(pull) => {
                self.pull_documents(pull, &identity, &known_roots, bypass_dac)
                    .await?
            }
            None => BrowserSyncResponse::default(),
        };
        response.refused = refused;
        Ok(response)
    }
}

/// Report a refused document, and say so in the log as well: a refusal the
/// caller decides to ignore must still be visible to whoever runs the node.
fn refusal(doc_id: &str, reason: String) -> BrowserSyncRefusal {
    tracing::warn!(%doc_id, %reason, "browser sync refused a pushed document");
    BrowserSyncRefusal {
        doc_id: doc_id.to_string(),
        reason,
    }
}

fn map_engine_error(error: db::merge::BrowserSyncError) -> BrowserSyncError {
    match error {
        db::merge::BrowserSyncError::Invalid(message)
        | db::merge::BrowserSyncError::TooLarge(message)
        | db::merge::BrowserSyncError::Merge(message) => BrowserSyncError::InvalidInput(message),
        db::merge::BrowserSyncError::Storage(message) => BrowserSyncError::Internal(message),
        other => BrowserSyncError::Internal(other.to_string()),
    }
}

/// Resolve the push's grants before anything in the request is applied.
///
/// Who may grant stays with `add_actor_relationship`. This refuses only what
/// that call cannot express: a caller with no DID, a collection with no policy,
/// a target that is neither an actor nor the wildcard, and the immutable
/// `owner` relation.
fn prepare_relationships(
    document: &BrowserSyncDocument,
    collection: &db::Collection,
    identity: &Identity,
) -> BrowserSyncResult<Vec<PendingRelationship>> {
    if document.relationships.is_empty() {
        return Ok(Vec::new());
    }
    if document.relationships.len() > MAX_SYNC_RELATIONSHIPS_PER_DOCUMENT {
        return Err(BrowserSyncError::InvalidInput(format!(
            "sync document {} exceeds {MAX_SYNC_RELATIONSHIPS_PER_DOCUMENT} relationships",
            document.doc_id
        )));
    }
    if identity.did().is_none() {
        return Err(BrowserSyncError::Forbidden(format!(
            "sync relationships on document {} require an authenticated caller",
            document.doc_id
        )));
    }
    if collection.schema().policy.is_none() {
        return Err(BrowserSyncError::InvalidInput(format!(
            "collection '{}' has no policy to grant relationships under",
            document.collection_id
        )));
    }

    let mut prepared = Vec::with_capacity(document.relationships.len());
    for relationship in &document.relationships {
        validate_id("relation", &relationship.relation)?;
        validate_id("relationship target", &relationship.target)?;
        if relationship.relation == "owner" {
            return Err(BrowserSyncError::InvalidInput(
                "cannot add owner relation".into(),
            ));
        }
        let target = if relationship.target == "*" {
            identity::Did::wildcard()
        } else {
            identity::Did::try_from(relationship.target.clone()).map_err(|error| {
                BrowserSyncError::InvalidInput(format!(
                    "relationship target '{}' is not an actor DID: {error}",
                    relationship.target
                ))
            })?
        };
        prepared.push(PendingRelationship {
            relation: relationship.relation.clone(),
            target,
        });
    }
    Ok(prepared)
}

fn validate_optional_id(name: &str, value: Option<&str>) -> BrowserSyncResult<()> {
    match value {
        Some(value) => validate_id(name, value),
        None => Ok(()),
    }
}

fn validate_id(name: &str, value: &str) -> BrowserSyncResult<()> {
    if value.is_empty() || value.len() > MAX_SYNC_ID_BYTES {
        return Err(BrowserSyncError::InvalidInput(format!(
            "{name} has invalid length {}",
            value.len()
        )));
    }
    Ok(())
}

fn document_payload_bytes(document: &BrowserSyncDocument) -> usize {
    document
        .blocks
        .iter()
        .map(|block| block.data.len() / 2)
        .sum()
}

fn same_roots(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left.iter().all(|root| right.contains(root))
        && right.iter().all(|root| left.contains(root))
}
