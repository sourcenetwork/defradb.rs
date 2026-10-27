//! Read checks for chunk documents outside a query plan. A chunk is readable
//! only while its parent is live, lists it, and is readable.

use acp::Identity;
use identity::Did;
use schema::CollectionVersion;

use super::{DocFetcher, QueryRunner};
use crate::error::Result;
use crate::txn::TransactionRegistry;

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    pub(crate) async fn chunk_readable(
        &self,
        fetcher: &dyn DocFetcher,
        identity: &Identity,
        caller: Option<&Did>,
        chunks_col: &CollectionVersion,
        doc_id: &str,
    ) -> Result<bool> {
        let Some(source) = &chunks_col.chunk_of else {
            return Ok(false);
        };
        let Ok(parent) = self.get_collection(&source.collection).await else {
            return Ok(false);
        };
        let Some(chunked) = parent.chunked_field(&source.field_name) else {
            return Ok(false);
        };
        let Some(parent_id) =
            crate::plan::live_parent_of(fetcher, &chunks_col.name, &parent.name, chunked, doc_id)
                .await?
        else {
            return Ok(false);
        };
        if let Some(policy) = &parent.policy {
            let allowed = crate::txn::check_doc_read_access_as(
                &self.context(),
                self.acp.as_ref(),
                self.acp_registration_projection.as_deref(),
                identity,
                &policy.id,
                &policy.resource_name,
                &parent.collection_id,
                parent.is_branchable,
                &parent_id,
                self.retry_inputs.as_ref(),
            )
            .await?;
            if !allowed {
                return Ok(false);
            }
        }
        self.app_may_read(caller, &parent, &parent_id).await
    }
}
