//! Lens transform registration and listing inside a transaction.

use super::*;
use crate::definition::migration::committed_lenses::PreparedLens;
use lens::{LensConfig, LensModule, TransformId};
use storage::corekv::OnCommit;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// [`Self::add_lens_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn add_lens_in_txn(&self, txn_id: &str, config: LensConfig) -> Result<TransformId> {
        self.add_lens_in_txn_as(&RequestContext::ambient(), txn_id, config)
            .await
    }

    /// Add a standalone lens within a transaction.
    ///
    /// The lens is visible only within this transaction until commit. Its blocks
    /// and config are written into the transaction, so the lens is durable with
    /// its commit; the commit then registers it in the database's lens store.
    pub async fn add_lens_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        config: LensConfig,
    ) -> Result<TransformId> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::LensCreate)
            .await?;
        let lens = PreparedLens::new(config, self.db.pool()).await?;
        let db = self.db.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn_lens_store = owned.lens_store();
                if txn_lens_store.has_transform(&lens.id) {
                    return Ok(lens.id);
                }
                txn_lens_store
                    .add_with_id(lens.id.clone(), lens.config.clone())
                    .await
                    .map_err(Error::from)?;
                let txn = owned.alone()?;
                lens.write(&txn.blockstore()?, &txn.systemstore()?).await?;

                let transform_id = lens.id.clone();
                let config = lens.config;
                txn.on_commit(OnCommit::Async(Box::new(move || {
                    let db = db.clone();
                    let transform_id = transform_id.clone();
                    let config = config.clone();
                    Box::pin(async move {
                        db.register_committed_lens(transform_id, config).await;
                    })
                })))?;

                Ok(lens.id)
            })
        })
        .await
    }

    /// [`Self::list_lenses_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn list_lenses_in_txn(
        &self,
        txn_id: &str,
    ) -> Result<rapidhash::RapidHashMap<String, Vec<LensModule>>> {
        self.list_lenses_in_txn_as(&RequestContext::ambient(), txn_id)
            .await
    }

    /// List all lenses visible within a transaction.
    pub async fn list_lenses_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
    ) -> Result<rapidhash::RapidHashMap<String, Vec<LensModule>>> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::LensList)
            .await?;
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                owned
                    .lens_store()
                    .list()
                    .await
                    .map_err(|e| Error::Lens(e.to_string()))
            })
        })
        .await
    }
}
