//! `client collection prune-history`.

use super::PruneHistoryArgs;
use crate::commands::client::http_client::HttpClient;
use crate::commands::client::ClientContext;
use crate::error::{Error, Result};

impl PruneHistoryArgs {
    /// Prune the collection's history and print `{"count": pruned}`.
    pub async fn execute(&self, ctx: &ClientContext, name: Option<&str>) -> Result<()> {
        let collection = name.ok_or_else(|| {
            Error::MissingInput("--collection-name is required for prune-history".to_string())
        })?;
        let client = HttpClient::new(&ctx.url, &ctx.edge)?
            .with_auth_token(ctx.auth_token.clone())
            .with_verbose(ctx.verbose);
        let response = client.collection_prune_history(collection).await?;
        println!("{}", serde_json::to_string(&response)?);
        Ok(())
    }
}
