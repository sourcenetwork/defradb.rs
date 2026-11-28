//! The node's report of owed relationship changes a provider refuses for good.

use std::sync::Arc;

/// Publishes every owed change the provider refuses for good on the node's
/// event bus.
pub(crate) struct PublishRefusals(pub(crate) Arc<dyn events::Bus>);

impl acp::OwedRefusalListener for PublishRefusals {
    fn refused(&self, change: &acp::OwedChange, reason: &str, kept: bool) {
        let tuple = &change.tuple;
        self.0.publish(events::Message::acp_owed_change_refused(
            events::AcpOwedChangeRefusedData {
                seq: change.seq,
                actor: change.actor.to_string(),
                policy_id: tuple.policy_id.clone(),
                resource: tuple.resource.clone(),
                object_id: tuple.object.clone(),
                relation: tuple.relation.clone(),
                subject: acp::Subject::from(tuple.subject.clone()).to_string(),
                delete: change.op == acp::OwedOp::Delete,
                transfer: change.op == acp::OwedOp::Transfer,
                reason: reason.to_owned(),
                parked: kept,
            },
        ));
    }
}
