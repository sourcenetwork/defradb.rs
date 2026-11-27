//! The inputs a create in a gated collection prepares outside its
//! transaction: the decision on `create`, on the provider's certified state,
//! and a policy only the remote provider holds.

use std::sync::Arc;

use storage::corekv::Store;

use crate::database::DB;

/// Decide `create` for the actor a [`query::prepared::CreateDecisionRequest`]
/// names, which must be the one `context` acts as, with its credential.
pub(super) async fn resolve_create_decision<S>(
    db: &DB<S>,
    context: &defra_core::RequestContext,
    request: &query::prepared::PreparedInputRequest,
) -> query::error::Result<Arc<[u8]>>
where
    S: Store + 'static,
{
    let (actor, bearer, policy, _resource, create, derived): query::prepared::CreateDecisionRequest =
        serde_json::from_slice(request.exact_request())
            .map_err(|_| query::error::QueryError::execution("invalid prepared create decision"))?;
    if context.creator() != Some(actor.as_str()) || context.bearer_token_for(&actor) != bearer {
        return Err(query::error::QueryError::permission_denied(
            "prepared create decision differs from its creator",
        ));
    }
    let provider = db
        .document_acp()
        .ok_or_else(|| query::error::QueryError::execution("missing document ACP"))?;
    let actor = identity::Did::new(actor)
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    let bound = acp::bound_to_request(provider, context);
    let allowed = acp::authorize_create_through(bound.as_ref(), &actor, &policy, &create, &derived)
        .await
        .map_err(query::error::QueryError::from_acp)?;
    let bytes = serde_json::to_vec(&allowed)
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    Ok(bytes.into())
}

/// Fetch the policy a request names from the remote provider, keeping it in
/// the node's mirror, outside any transaction.
pub(super) async fn resolve_dac_policy<S>(
    db: &DB<S>,
    request: &query::prepared::PreparedInputRequest,
) -> query::error::Result<Arc<[u8]>>
where
    S: Store + 'static,
{
    let policy_id = std::str::from_utf8(request.exact_request())
        .map_err(|_| query::error::QueryError::execution("invalid prepared policy request"))?;
    let policies = db
        .dac_policies()
        .ok_or_else(|| query::error::QueryError::execution("missing document policies"))?;
    let policy = policies
        .get(policy_id)
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    let bytes = serde_json::to_vec(&policy)
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    Ok(bytes.into())
}
