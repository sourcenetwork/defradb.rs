//! The create gate of a document in a collection whose policy restricts who
//! may add documents to it (the `create` of its resource's `collection`
//! section).
//!
//! The relationships a document carries through its `@policy(relation:)`
//! fields are derived here and registered with it: an external provider
//! (vera) checks `create` over them and stores them with the owner in one
//! execution, so the check runs where the registration does, after the
//! commit. The local engine ignores them. What this node refuses itself is a
//! create without an identity in a gated collection under a provider that
//! checks `create`: nobody could be checked.

use defra_core::RequestContext;
use document::Document;
use storage::corekv::Store;

use crate::collection::create_gate::CreateGate;
use crate::collection::Collection;
use crate::database::DB;

/// The create gate of `doc`: the relationships it is registered with and
/// whether its collection gates creates, or the refusal of its create.
pub(super) async fn create_gate<S>(
    db: &DB<S>,
    collection: &Collection,
    doc: &Document,
    context: &RequestContext,
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
    let policy = policies
        .get(&described.id)
        .await
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    let gate = crate::collection::create_gate::create_gate(schema, doc, policy.as_ref())
        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
    // A provider that registers inside the create's transaction is the
    // local engine, which does not gate creates.
    let checks_create = !provider.supports_transactions();
    if gate.gated && checks_create && context.creator().is_none() && !context.dac_bypass() {
        return Err(query::error::QueryError::from_acp(
            acp::Error::AnonymousCreate {
                collection: schema.name.clone(),
            },
        ));
    }
    Ok(gate)
}
