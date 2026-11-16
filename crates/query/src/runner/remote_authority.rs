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
//! name exactly that registration. An update or a delete owes nothing, so a
//! delegation bound to one request has nothing to name there and does not
//! count.
//!
//! The check runs before the statement writes anything. Modeled by
//! proofs/tla/VerifiedRemoteWrite.tla.

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
                provider.command_authority(
                    signer,
                    acp::ProviderCommand::RegisterObject {
                        policy_id: &policy.id,
                        resource: &policy.resource_name,
                        object,
                    },
                )
            }
            MutationType::Update | MutationType::Delete | MutationType::Truncate => {
                provider.owed_authority(signer)
            }
        };
        authority.map_err(|error| QueryError::WriteAuthorityUnavailable(error.to_string()))?;
    }
    Ok(())
}
