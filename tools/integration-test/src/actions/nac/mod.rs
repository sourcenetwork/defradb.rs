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

/// The actor a relationship names: a DID or `*` as written, or a client
/// identity's DID. A node's identity is not offered yet.
fn target_actor(
    op: &str,
    actor: Option<&str>,
    seed: Option<u64>,
    node: Option<&str>,
) -> Result<String, Failure> {
    if node.is_some() {
        return Err(Failure::unsupported(format!(
            "{op} argument targetIdentityNode"
        )));
    }
    Ok(match (actor, seed) {
        (Some(actor), _) => actor.to_owned(),
        (None, Some(seed)) => crate::scenario::identity::did(seed),
        (None, None) => String::new(),
    })
}
