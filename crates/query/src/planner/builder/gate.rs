//! The read gate every join child passes.

use schema::CollectionVersion;

use super::Planner;
use crate::plan::PermissionFilterNode;
use crate::planner::PlanNode;

/// A join's child rows after the reader's read gate.
///
/// Only `Planner::gate_child` makes one, so a join never sees a related
/// row the reader may not read, whether it renders, filters, counts or sorts by it.
pub struct GatedChild {
    plan: Box<dyn PlanNode>,
    open: bool,
}

impl GatedChild {
    /// Whether no gate applies, so the join may read the collection directly.
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn into_plan(self) -> Box<dyn PlanNode> {
        self.plan
    }

    #[cfg(test)]
    pub(crate) fn ungated(plan: Box<dyn PlanNode>) -> Self {
        Self { plan, open: true }
    }
}

impl Planner {
    /// Gate `plan`, rows of `collection`, as a join child.
    pub(in crate::planner) fn gate_child(
        &self,
        plan: Box<dyn PlanNode>,
        collection: &CollectionVersion,
    ) -> GatedChild {
        let (acp, app) = self.read_checks(collection);
        let open = acp.is_none() && app.is_none();
        let plan = PermissionFilterNode::wrap_with_projection_and_inputs(
            plan,
            acp,
            app,
            self.acp_registration_projection.clone(),
            self.retry_inputs.clone(),
        );
        GatedChild { plan, open }
    }
}
