//! Storing and listing standalone lenses, outside any explicit transaction.

use lens::{LensConfig, LensModule, TransformId};
use rapidhash::RapidHashMap;
use storage::corekv::Store;

use crate::error::Result;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Store `config` as a standalone lens for `context`'s caller, Go's
    /// `AddLens`: its content ID, the same for the same lens.
    pub async fn add_lens_as(
        &self,
        context: &defra_core::RequestContext,
        config: LensConfig,
    ) -> Result<TransformId> {
        self.check_node_access_as(context, acp::nac::NodePermission::LensCreate)
            .await?;
        self.add_lens(config).await
    }

    /// Every stored lens by content ID for `context`'s caller, Go's
    /// `ListLenses`.
    pub async fn list_lenses_as(
        &self,
        context: &defra_core::RequestContext,
    ) -> Result<RapidHashMap<String, Vec<LensModule>>> {
        self.check_node_access_as(context, acp::nac::NodePermission::LensList)
            .await?;
        Ok(self.lens_store().list().await?)
    }
}
