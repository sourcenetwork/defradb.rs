//! The create gate of a document in a collection whose policy restricts who
//! may add documents to it (the `create` of its resource's `collection`
//! section).
//!
//! The relationships a document carries through its `@policy(relation:)`
//! fields are derived here and registered with it. Under a provider that
//! cannot join the transaction (vera), `create` is decided before the
//! transaction on the provider's certified state, a prepared input this
//! reads, and a refused create fails here and writes nothing; the provider
//! decides again at registration, after the commit. Under the local engine
//! the create's own transaction decides it, where it registers
//! (`register_created_doc`). A create without an identity in a gated
//! collection is refused: nobody could be checked.
//!
//! Nothing remote is fetched here: a policy the node has not mirrored is a
//! prepared input as well.

use defra_core::RequestContext;
use document::Document;
use query::prepared::PreparedInputMemo;
use storage::corekv::Store;

use crate::collection::create_gate::CreateGate;
use crate::collection::Collection;
use crate::database::DB;

/// The create gate of `doc`, whose ID is `doc_id`: the relationships it is
/// registered with and whether its collection gates creates, or the refusal
/// of its create.
pub(super) async fn create_gate<S>(
    db: &DB<S>,
    collection: &Collection,
    doc: &Document,
    doc_id: &str,
    context: &RequestContext,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<CreateGate>
where
    S: Store + 'static,
{
    let schema = collection.schema();
    let (Some(described), Some(provider)) = (schema.policy.as_ref(), db.document_acp()) else {
        return Ok(CreateGate::default());
    };
    // Only a governed collection binds fields to policy relations, and only
    // its creates are gated.
    if schema.governance_root.is_none() {
        return Ok(CreateGate::default());
    }
    let Some(policies) = db.dac_policies() else {
        return Ok(CreateGate::default());
    };
    let remote = !provider.supports_transactions();
    let policy = match policies
        .get_local(&described.id)
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?
    {
        Some(policy) => Some(policy),
        None if remote => prepared(inputs)?.dac_policy(&described.id)?,
        None => None,
    };
    let gate = crate::collection::create_gate::create_gate(schema, doc, policy.as_ref())
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    if !gate.gated || context.dac_bypass() {
        return Ok(gate);
    }
    let Some(creator) = context.creator() else {
        return Err(query::error::QueryError::from_acp(
            acp::Error::AnonymousCreate {
                collection: schema.name.clone(),
            },
        ));
    };
    if !remote {
        return Ok(gate);
    }
    let create = gate.create.as_deref().unwrap_or("*");
    let actor = identity::Did::new(creator)
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    let allowed = prepared(inputs)?.create_decision(
        &actor,
        context,
        &described.id,
        &described.resource_name,
        create,
        &gate.derived,
    )?;
    if !allowed {
        return Err(query::error::QueryError::from_acp(
            acp::Error::CreateNotAuthorized {
                doc_id: doc_id.to_owned(),
                reason: format!(
                    "{actor} may not create {}/{doc_id} in its collection",
                    described.resource_name
                ),
            },
        ));
    }
    Ok(gate)
}

/// The request's prepared inputs. Every write the node runs carries them; a
/// create without them cannot be decided and fails loud.
fn prepared(inputs: Option<&PreparedInputMemo>) -> query::error::Result<&PreparedInputMemo> {
    inputs.ok_or_else(|| {
        query::error::QueryError::execution(
            "a create in a gated collection needs its prepared inputs",
        )
    })
}
