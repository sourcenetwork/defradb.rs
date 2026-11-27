//! A shared transport collection restored on a device from its sentinel
//! proof.

use defra_core::RequestContext;
use schema::CollectionVersion;

use super::NodeCore;
use crate::Result;

impl NodeCore<'_> {
    pub(crate) async fn restore_shared_transport_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
        selection: acp::TransportSentinelQuery,
    ) -> Result<Vec<CollectionVersion>> {
        if context.identity() != Some(selection.device.as_str()) || context.txn().is_some() {
            return Err(acp::Error::PermissionDenied("shared transport restoration requires its device identity and no caller transaction".into()).into());
        }
        if self.node.database().document_acp().is_none() {
            return Err(acp::Error::PermissionDenied(
                "shared transport restoration requires document access control".into(),
            )
            .into());
        }
        let created = self
            .node
            .schema_ops
            .add_collection(
                &context
                    .clone()
                    .with_shared_transport_sentinel(selection.clone()),
                sdl,
            )
            .await?;
        if created.len() != 1 || created[0].collection_id != selection.child {
            return Err(acp::Error::PermissionDenied(
                "restored collection identity does not match the proof".into(),
            )
            .into());
        }
        if !self
            .node
            .document_acp()
            .verify_transport_sentinel(&selection)
            .await?
        {
            return Err(acp::Error::PermissionDenied(
                "shared transport sentinel is no longer admitted".into(),
            )
            .into());
        }
        Ok(created)
    }
}
