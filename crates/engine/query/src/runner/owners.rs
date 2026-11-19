use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use rapidhash::RapidHashSet;

use super::turn::Turned;
use super::{DocFetcher, QueryRunner};
use crate::error::{QueryError, Result};
use crate::executor::QueryRequest;
use crate::owners::{
    DocumentAccessDecision, DocumentAccessMode, DocumentOwnerObservation, MAX_OWNER_OBSERVATIONS,
};
use crate::prepared::{PreparedInputKind, PreparedInputMemo, PreparedInputRequest};
use crate::txn::{
    txn_job, AcpRegistrationProjection, ObservedOwners, TransactionHandle, TransactionRegistry,
};

pub(super) async fn named_access_decision(
    acp: &dyn acp::DocumentACP,
    actor: &Did,
    permission: &str,
    policy: &schema::PolicyDescription,
    doc_id: &str,
    node: Option<bool>,
) -> Result<DocumentAccessDecision> {
    let Some(node_allowed) = node else {
        return Ok(DocumentAccessDecision::Unavailable);
    };
    match acp
        .check_named_permission(actor, permission, &policy.id, &policy.resource_name, doc_id)
        .await
    {
        Ok(named_allowed) if node_allowed && named_allowed => Ok(DocumentAccessDecision::Allowed),
        Ok(_) => Ok(DocumentAccessDecision::Denied),
        Err(acp::Error::UnsupportedNamedPermission) => Ok(DocumentAccessDecision::Unavailable),
        Err(error) => Err(QueryError::execution(error.to_string())),
    }
}

fn validate_observation_ids(collection: &str, doc_ids: &[String]) -> Result<()> {
    let mut name = collection.chars();
    if !name
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !name.all(|c| c.is_ascii_alphanumeric() || c == '_')
        || collection.len() > 1024
        || doc_ids.is_empty()
        || doc_ids.len() > MAX_OWNER_OBSERVATIONS
        || doc_ids.iter().any(|id| id.is_empty() || id.len() > 1024)
    {
        return Err(QueryError::parse(format!(
            "choose a collection and between 1 and {MAX_OWNER_OBSERVATIONS} known document IDs"
        )));
    }
    Ok(())
}

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    pub(super) async fn observe_access(
        &self,
        context: RequestContext,
        handle: &TransactionHandle,
        collection: &str,
        doc_ids: &[String],
        actor: Did,
        mode: DocumentAccessMode,
    ) -> Result<Vec<DocumentAccessDecision>> {
        if actor.is_wildcard() {
            return Err(QueryError::execution("an explicit actor DID is required"));
        }
        if let DocumentAccessMode::DelegatedRead = mode {
            return Ok(self
                .observe_owners(context, handle, collection, doc_ids, Some(actor), true)
                .await?
                .into_iter()
                .map(|row| {
                    if row.visible {
                        DocumentAccessDecision::Allowed
                    } else {
                        DocumentAccessDecision::Unavailable
                    }
                })
                .collect());
        }
        let (named, document, node_permission) = match mode {
            DocumentAccessMode::Named {
                permission,
                node_permission,
            } => (Some(permission), None, node_permission),
            DocumentAccessMode::Document {
                permission,
                node_permission,
            } => (None, Some(permission), node_permission),
            DocumentAccessMode::DelegatedRead => unreachable!(),
        };
        validate_observation_ids(collection, doc_ids)?;
        let context = context.with_dac_bypass(false);
        let collection = collection.to_owned();
        let doc_ids = doc_ids.to_vec();
        let acp = self.acp.clone();
        let job = txn_job(move |turn| {
            Box::pin(async move {
                let ctx = turn
                    .context()
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                let acp = if acp.supports_transactions() {
                    match ctx.doc_fetcher().transaction_acp(acp).await? {
                        Some(acp) => acp,
                        None => {
                            return Ok(vec![DocumentAccessDecision::Unavailable; doc_ids.len()])
                        }
                    }
                } else {
                    acp
                };
                let provider = ctx.collection_provider().ok_or_else(|| {
                    QueryError::execution("transaction schema observation is unavailable")
                })?;
                let definition = provider
                    .get_collection(&collection)
                    .await?
                    .ok_or_else(|| QueryError::collection_not_found(&collection))?;
                let policy = definition.policy.as_ref().ok_or_else(|| {
                    QueryError::execution("collection has no document authorization policy")
                })?;
                let node = ctx
                    .doc_fetcher()
                    .check_bound_node_permission(&actor, node_permission, true)
                    .await?;
                let rows = ctx.doc_fetcher().get_by_ids(&collection, &doc_ids).await?;
                let missing: RapidHashSet<&str> =
                    rows.missing_ids().iter().map(String::as_str).collect();
                let mut result = Vec::with_capacity(doc_ids.len());
                for doc_id in &doc_ids {
                    if missing.contains(doc_id.as_str())
                        || !acp
                            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
                            .await
                            .map_err(|error| QueryError::execution(error.to_string()))?
                    {
                        result.push(DocumentAccessDecision::Unavailable);
                        continue;
                    }
                    let decision = match (&named, document) {
                        (Some(permission), _) => {
                            named_access_decision(
                                acp.as_ref(),
                                &actor,
                                permission,
                                policy,
                                doc_id,
                                node,
                            )
                            .await?
                        }
                        (_, Some(permission)) => match node {
                            None => DocumentAccessDecision::Unavailable,
                            Some(node_allowed) => {
                                let allowed = acp
                                    .check_doc_access(
                                        &acp::Identity::Authenticated(actor.clone()),
                                        permission,
                                        &policy.id,
                                        &policy.resource_name,
                                        doc_id,
                                    )
                                    .await
                                    .map_err(|error| QueryError::execution(error.to_string()))?;
                                if node_allowed && allowed {
                                    DocumentAccessDecision::Allowed
                                } else {
                                    DocumentAccessDecision::Denied
                                }
                            }
                        },
                        _ => unreachable!(),
                    };
                    result.push(decision);
                }
                Ok(result)
            })
        });
        self.registry
            .run_as(&context, handle, self.deadline(), job)
            .await
            .map_err(|error| QueryError::execution(error.to_string()))?
    }

    pub(super) async fn observe_owners(
        &self,
        context: RequestContext,
        handle: &TransactionHandle,
        collection: &str,
        doc_ids: &[String],
        identity: Option<identity::Did>,
        node_authorized: bool,
    ) -> Result<Vec<DocumentOwnerObservation>> {
        validate_observation_ids(collection, doc_ids)?;
        let context = if node_authorized {
            context.with_dac_bypass(false)
        } else {
            context.or_ambient()
        };
        let nac_identity =
            if node_authorized {
                Some(
                    Did::new(context.identity().ok_or_else(|| {
                        QueryError::execution("transaction principal is required")
                    })?)
                    .map_err(|error| QueryError::execution(error.to_string()))?,
                )
            } else {
                None
            };
        let identity = match identity {
            Some(identity) => Some(identity),
            None => context
                .identity()
                .map(Did::new)
                .transpose()
                .map_err(|error| QueryError::execution(error.to_string()))?,
        };
        let ids = serde_json::to_string(doc_ids)
            .map_err(|error| QueryError::execution(error.to_string()))?;
        let request = QueryRequest::new(format!("{{ {collection}(docID: {ids}) {{ _docID }} }}"))
            .with_identity(identity)
            .with_context(context.clone());
        let runner = if node_authorized {
            self.for_request(context.clone())
        } else {
            self.for_query(&request)
        };
        // The inputs every attempt of this observation shares: a provider
        // answer an attempt misses is fetched here, outside the transaction,
        // and the observation runs again with it.
        let inputs = PreparedInputMemo::default();
        loop {
            let job = owner_job(
                runner.clone(),
                request.clone(),
                collection.to_owned(),
                doc_ids.to_vec(),
                nac_identity.clone(),
                context.clone(),
                inputs.clone(),
            );
            match self
                .registry
                .run_as(&context, handle, self.deadline(), job)
                .await
                .map_err(|error| QueryError::execution(error.to_string()))?
            {
                Err(QueryError::NeedsInput(input)) => {
                    inputs
                        .answer_wave(&input, |wave| runner.resolve_wave(wave))
                        .await?
                }
                answered => return answered,
            }
        }
    }
}

/// Where an owner observation reads owners from.
enum OwnerSource {
    /// A provider that shares the transaction: its owner is the snapshot's.
    Bound(Arc<dyn acp::DocumentACP>),
    /// A provider that cannot join it ([`remote_owner`]).
    Remote {
        acp: Arc<dyn acp::DocumentACP>,
        projection: Option<Arc<AcpRegistrationProjection>>,
        observed: Arc<ObservedOwners>,
    },
}

const UNBOUND_OWNER: &str = "transaction-bound authorization is unavailable for owner observations";

/// One attempt of an owner observation, run where the transaction lives. A
/// provider answer it lacks ends it with [`QueryError::NeedsInput`].
fn owner_job<F, R>(
    runner: QueryRunner<F, R>,
    request: QueryRequest,
    collection: String,
    doc_ids: Vec<String>,
    nac_identity: Option<Did>,
    authorization: RequestContext,
    inputs: PreparedInputMemo,
) -> crate::txn::TxnJob<Result<Vec<DocumentOwnerObservation>>>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    let node_authorized = nac_identity.is_some();
    let requester = request.identity.clone();
    txn_job(move |turn| {
        Box::pin(async move {
            if !authorization.authorization_valid_now() {
                return Err(QueryError::permission_denied(
                    "request authorization expired before the transaction turn",
                ));
            }
            let ctx = turn
                .context()
                .map_err(|error| QueryError::execution(error.to_string()))?;
            let source = match ctx
                .doc_fetcher()
                .transaction_acp(runner.acp.clone())
                .await?
            {
                Some(acp) => OwnerSource::Bound(acp),
                // A remote provider's answer is confirmed at commit, so only a
                // transaction that keeps answers for its commit can take one.
                None if !node_authorized && !runner.acp.supports_transactions() => {
                    match ctx.observed_owners() {
                        Some(observed) => OwnerSource::Remote {
                            acp: runner.acp.clone(),
                            projection: ctx.acp_registration_projection(),
                            observed,
                        },
                        None => return Err(QueryError::execution(UNBOUND_OWNER)),
                    }
                }
                None => return Err(QueryError::execution(UNBOUND_OWNER)),
            };
            let provider = ctx.collection_provider().ok_or_else(|| {
                QueryError::execution("transaction schema observation is unavailable")
            })?;
            let definition = provider
                .get_collection(&collection)
                .await?
                .ok_or_else(|| QueryError::collection_not_found(&collection))?;
            drop(ctx);
            // The ordinary read path owns node, application, document and live
            // membership gates. Both reads stay on this one transaction turn.
            let delegation = match (&source, nac_identity) {
                (OwnerSource::Bound(acp), Some(identity)) => {
                    Some(super::turn::DelegatedAuthority {
                        identity,
                        acp: acp.clone(),
                    })
                }
                _ => None,
            };
            let response = match runner
                .run_turn_with_delegation(turn, request, inputs.clone(), None, delegation)
                .await
            {
                Turned::Done(response) => response,
                Turned::NeedsInput(input, _) => return Err(QueryError::NeedsInput(input)),
            };
            if let Some(error) = response.errors.first() {
                return Err(QueryError::execution(error.message.clone()));
            }
            let rows = response
                .data
                .as_ref()
                .and_then(|data| data.get(&collection))
                .and_then(|rows| rows.as_array());
            // Node denial uses an empty data object on the normal read path.
            if node_authorized && rows.is_none() {
                return Err(QueryError::execution(
                    "transaction principal is not authorized for document reads",
                ));
            }
            let visible: RapidHashSet<&str> = rows
                .into_iter()
                .flatten()
                .filter_map(|row| row.get("_docID").and_then(|id| id.as_str()))
                .collect();
            let mut result = Vec::with_capacity(doc_ids.len());
            let mut missed = None;
            for doc_id in doc_ids {
                let readable = visible.contains(doc_id.as_str());
                let owner = match (&definition.policy, &source) {
                    _ if !readable || node_authorized => None,
                    (Some(policy), OwnerSource::Bound(acp)) => {
                        let acp_error =
                            |error: acp::Error| QueryError::execution(error.to_string());
                        if acp
                            .is_doc_registered(&policy.id, &policy.resource_name, &doc_id)
                            .await
                            .map_err(acp_error)?
                        {
                            acp.get_doc_owner(&policy.id, &policy.resource_name, &doc_id)
                                .await
                                .map_err(acp_error)?
                        } else {
                            None
                        }
                    }
                    (
                        Some(policy),
                        OwnerSource::Remote {
                            acp,
                            projection,
                            observed,
                        },
                    ) => match remote_owner(
                        acp.as_ref(),
                        projection.as_deref(),
                        observed,
                        requester.as_ref(),
                        policy,
                        &doc_id,
                        &inputs,
                    )
                    .await
                    {
                        Ok(owner) => owner,
                        Err(QueryError::NeedsInput(input)) => {
                            missed.get_or_insert(input);
                            None
                        }
                        Err(error) => return Err(error),
                    },
                    (None, _) => None,
                };
                result.push(DocumentOwnerObservation {
                    doc_id,
                    visible: readable,
                    owner,
                });
            }
            match missed {
                Some(input) => Err(QueryError::NeedsInput(input)),
                None => Ok(result),
            }
        })
    })
}

/// The owner of `doc_id` on a provider that cannot join the transaction:
///
/// - a registration this transaction, or an earlier committed one, still owes
///   names its creator: the node's own claim, read from the transaction's view
///   as a validated point read, so a settle that commits first conflicts;
/// - otherwise the provider's owner, read at the edge as a prepared input and
///   kept in `observed`, which the commit confirms with the provider
///   ([`QueryRunner::commit_confirming`]).
///
/// A provider answer this attempt lacks is noted in `inputs` and ends the
/// attempt with [`QueryError::NeedsInput`]. Modeled by
/// proofs/lean/RemoteOwnerAnswer.lean (`answer`).
async fn remote_owner(
    acp: &dyn acp::DocumentACP,
    projection: Option<&AcpRegistrationProjection>,
    observed: &ObservedOwners,
    requester: Option<&Did>,
    policy: &schema::PolicyDescription,
    doc_id: &str,
    inputs: &PreparedInputMemo,
) -> Result<Option<Did>> {
    let owed = match projection {
        Some(projection) => projection
            .owed_creator(&policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(|error| QueryError::execution(error.to_string()))?,
        None => None,
    };
    if owed.is_some() {
        return Ok(owed);
    }
    let bearer = requester.and_then(|did| acp.bearer_token_for(did.as_str()));
    let exact_request = serde_json::to_vec(&(
        requester.map(Did::as_str).unwrap_or("anonymous"),
        "owner",
        &policy.id,
        &policy.resource_name,
        doc_id,
        bearer,
    ))
    .map_err(|error| QueryError::execution(error.to_string()))?;
    let request = PreparedInputRequest::new(PreparedInputKind::AcpDecision, exact_request);
    let Some(answer) = inputs.get_or_want(&request) else {
        return Err(QueryError::NeedsInput(request));
    };
    let owner: Option<Did> = serde_json::from_slice(&answer).map_err(|error| {
        QueryError::execution(format!("cached ACP response is invalid: {error}"))
    })?;
    observed.record(request, answer);
    Ok(owner)
}
