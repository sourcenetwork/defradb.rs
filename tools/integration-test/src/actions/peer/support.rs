//! What the multi-node actions share: the request context, errors, and the
//! addresses of the nodes they name.

use std::time::Duration;

use defra_core::RequestContext;

use crate::scenario::Failure;
use crate::test_case::State;

/// How long Go's harness waits for a replicator's first push.
pub const REPLICATOR_COMPLETED: Duration = Duration::from_secs(1);
/// How long Go's harness waits for each local update event.
pub const UPDATE_EVENT: Duration = Duration::from_secs(1);
/// How long Go's harness waits for each merge event.
pub const MERGE_EVENT: Duration = Duration::from_secs(30);

pub fn ambient() -> RequestContext {
    RequestContext::ambient()
}

pub fn failed<E>(error: E) -> Failure
where
    E: std::fmt::Display,
{
    Failure::operation(error.to_string())
}

/// Every P2P address of the nodes `targets` names, in order.
pub fn addresses_of(state: &State, targets: &[&str]) -> Result<Vec<String>, Failure> {
    let mut addresses = Vec::new();
    for target in targets {
        addresses.extend(
            state
                .node(target)?
                .peer_info()
                .map_err(Failure::operation)?,
        );
    }
    Ok(addresses)
}
