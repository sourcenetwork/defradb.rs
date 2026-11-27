//! `client collection prune-history`.

use super::PruneHistoryArgs;
use crate::commands::client::document::write_client;
use crate::commands::client::ClientContext;
use crate::error::{Error, Result};

impl PruneHistoryArgs {
    /// Prune the collection's history and print `{"count": pruned}`.
    pub async fn execute(&self, ctx: &ClientContext, name: Option<&str>) -> Result<()> {
        let collection = name.ok_or_else(|| {
            Error::MissingInput("--collection-name is required for prune-history".to_string())
        })?;
        let (client, txn) = write_client(ctx)?;
        let response = client
            .collection_prune_history(txn.as_deref(), collection)
            .await?;
        println!("{}", serde_json::to_string(&response)?);
        Ok(())
    }
}
