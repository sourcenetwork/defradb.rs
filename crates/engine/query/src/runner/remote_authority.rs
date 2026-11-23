//! The authority a verified write needs on a document ACP that cannot join
//! its transaction.
//!
//! A local ACP authorizes a verified write inside the write's transaction. A
//! remote provider cannot: what the write needs from it is recorded in the
//! transaction as owed and sent after the commit (see `acp::outbox` and
//! `acp::pending`). Its authority counts as bound when the node can send
//! what is owed as the write's signer: with a key it holds for the signer,
//! or with the delegation the request carries for it. A create owes the
//! registration of its document, so a delegation bound to one request must
//! name exactly that registration.
//!
//! An update or a delete owes nothing ([`edit_credential`]). Its blocks are
//! signed by its signer, so the credential only says who is asking, and the
//! signer's permission on the document decides: the one check every update
//! and delete makes (`QueryRunner::authorize_mutation`), which on a remote
//! provider asks the gate of what is owed on this node and then the
//! provider. A delegation bound to one request has nothing to name there and
//! is refused.
//!
//! The check runs before the statement writes anything. Modeled by
//! proofs/tla/VerifiedRemoteWrite.tla and
//! proofs/lean/VerifiedRemoteAuthority.lean.

use std::sync::Arc;

use crate::collection_provider::CollectionProvider;
use crate::error::{QueryError, Result};
use crate::executor::QueryRequest;
use crate::mapper::{Mutation, MutationType};

use super::DocFetcher;

/// Whether the verified write `request` stages may run on `provider`, a
/// document ACP that did not join the transaction `fetcher` reads through.
pub(super) async fn remote_write_authority(
    fetcher: &dyn DocFetcher,
    provider: &Arc<dyn acp::DocumentACP>,
    collections: &dyn CollectionProvider,
    request: &QueryRequest,
    mutations: &[Mutation],
) -> Result<()> {
    // The handle is dropped at once: a savepoint needs every view of the
    // transaction gone.
    if fetcher
        .transaction_outbox(Arc::clone(provider))
        .await?
        .is_none()
    {
        return Err(QueryError::execution(
            "transaction-bound document authorization is unavailable",
        ));
    }
    let signer = request.identity.as_ref().ok_or_else(|| {
        QueryError::WriteAuthorityUnavailable("the verified write names no signer".into())
    })?;
    for mutation in mutations {
        let Some(collection) = collections
            .get_collection(&mutation.collection_name)
            .await?
        else {
            return Err(QueryError::execution(
                "transaction-bound collection is unavailable",
            ));
        };
        let Some(policy) = collection.policy.as_ref() else {
            continue;
        };
        let authority = match mutation.mutation_type {
            MutationType::Add | MutationType::Upsert => {
                let object = request.context.verified_document().ok_or_else(|| {
                    QueryError::WriteAuthorityUnavailable(
                        "the verified write names no document".into(),
                    )
                })?;
                let mut registration = acp::PendingRegistration::new(
                    signer.clone(),
                    &policy.id,
                    &policy.resource_name,
                    object,
                );
                if let Some(field) = &policy.parent_field {
                    let [input] = mutation.create_input.as_slice() else {
                        return Err(QueryError::WriteAuthorityUnavailable(
                            "a verified registration must name one child".into(),
                        ));
                    };
                    let id = input
                        .get(field)
                        .and_then(|value| value.as_str())
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| {
                            QueryError::WriteAuthorityUnavailable(format!(
                                "missing parent document ID in '{field}'"
                            ))
                        })?;
                    registration.parent = Some(
                        provider
                            .registration_parent(&policy.id, &policy.resource_name, id)
                            .await
                            .map_err(refused)?,
                    );
                }
                provider
                    .command_authority(signer, registration.command())
                    .map(drop)
            }
            MutationType::Update | MutationType::Delete => {
                edit_credential(provider.as_ref(), signer, &request.context)
            }
            MutationType::Truncate => provider.owed_authority(signer).map(drop),
        };
        authority.map_err(refused)?;
    }
    Ok(())
}

/// The query error for a refused authority: a delegation that does not hold
/// at the provider's time keeps its window, so the caller can issue it again.
fn refused(error: acp::Error) -> QueryError {
    match error {
        acp::Error::DelegationOutsideWindow { window, .. } => QueryError::DelegationOutsideWindow {
            reason: error.to_string(),
            window,
        },
        other => QueryError::WriteAuthorityUnavailable(other.to_string()),
    }
}

/// Whether the credential of a verified update or delete by `signer` may
/// stand beside the permission that decides it. It owes the provider
/// nothing, so the credential only says who is asking:
///
/// - none: the signature on the blocks says it, where `provider` proves the
///   permission against its finalized state ([`acp::DocumentACP::proves_decisions`]).
///   A provider that proves nothing still needs a key the node holds;
/// - a delegation: it must be the signer's, to this node, now, as for any
///   command;
/// - a delegation bound to one request: refused, key or no key, since an
///   update or a delete sends no command for it to name.
fn edit_credential(
    provider: &dyn acp::DocumentACP,
    signer: &identity::Did,
    context: &defra_core::RequestContext,
) -> acp::Result<()> {
    let refused = |reason: &str| acp::Error::OwedAuthorityUnavailable {
        actor: signer.to_string(),
        reason: reason.to_owned(),
    };
    if context.bearer_names_request() {
        return Err(refused(
            "a delegation bound to one request names no command of an update or a delete",
        ));
    }
    if context.bearer_token_for(signer.as_str()).is_none() && provider.proves_decisions() {
        return Ok(());
    }
    provider.owed_authority(signer).map(drop)
}
