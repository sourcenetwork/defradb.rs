//! Txn-overlay ACP checker for the shared read-access rule.

use acp::read_access::{DocAccess, ObjectAccessChecker};
use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;

use super::context::{
    check_doc_access_with_projection, is_doc_registered_with_projection, AcpRegistrationProjection,
};

/// Object access through a transaction's overlay. The NAC `bypass-dac`
/// privilege is checked before this rule runs (see
/// [`super::check_doc_read_access_as`]), as Go's checkDocAccess checks
/// canDACBypass first on every path.
pub struct OverlayChecker<'a> {
    pub acp: &'a dyn DocumentACP,
    pub projection: Option<&'a AcpRegistrationProjection>,
    pub identity: &'a Identity,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ObjectAccessChecker for OverlayChecker<'_> {
    async fn object_access(
        &self,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> acp::Result<DocAccess> {
        if !is_doc_registered_with_projection(
            self.acp,
            self.projection,
            policy_id,
            resource_name,
            object_id,
        )
        .await?
        {
            return Ok(DocAccess {
                has_access: self.acp.unregistered_documents_are_public(),
                explicit: false,
            });
        }

        let has_access = check_doc_access_with_projection(
            self.acp,
            self.projection,
            self.identity,
            DocumentPermission::Read,
            policy_id,
            resource_name,
            object_id,
        )
        .await?;

        Ok(DocAccess {
            has_access,
            explicit: true,
        })
    }
}
