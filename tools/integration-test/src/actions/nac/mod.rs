mod add_actor_relationship;
mod delete_actor_relationship;
mod disable;
mod re_enable;
mod status;

pub use add_actor_relationship::AddNACActorRelationship;
pub use delete_actor_relationship::DeleteNACActorRelationship;
pub use disable::DisableNAC;
pub use re_enable::ReEnableNAC;
pub use status::GetNACStatus;

use crate::scenario::Failure;

/// The actor a relationship names: a DID or `*` as written, a client
/// identity's DID, or a node's identity DID.
pub(crate) fn target_actor(
    op: &str,
    actor: Option<&str>,
    seed: Option<u64>,
    node: Option<&str>,
    state: &crate::test_case::State,
) -> Result<String, Failure> {
    if let Some(node) = node {
        return state
            .node_identity_of(node)
            .map(crate::scenario::identity::node_did)
            .ok_or_else(|| {
                Failure::unsupported(format!(
                    "{op} argument targetIdentityNode: node {node} was started with no identity"
                ))
            });
    }
    Ok(match (actor, seed) {
        (Some(actor), _) => actor.to_owned(),
        (None, Some(seed)) => crate::scenario::identity::did(seed),
        (None, None) => String::new(),
    })
}
