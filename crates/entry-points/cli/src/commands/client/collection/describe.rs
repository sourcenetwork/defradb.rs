//! `collection describe`: the collection versions a selector picks.

use schema::{CollectionSelector, CollectionVersion};

use super::CollectionDescribeArgs;
use crate::commands::client::http_client::HttpClient;
use crate::commands::client::ClientContext;
use crate::error::Result;

impl CollectionDescribeArgs {
    /// Print the versions `selector` picks as a JSON array, each in Go's
    /// display form. With no selector flag, every active collection.
    pub async fn execute(&self, ctx: &ClientContext, selector: &CollectionSelector) -> Result<()> {
        let client = HttpClient::new(&ctx.url, &ctx.edge)?
            .with_auth_token(ctx.auth_token.clone())
            .with_verbose(ctx.verbose);
        let versions = client
            .get_collections(ctx.tx_id.as_deref(), selector)
            .await?;
        let display = versions
            .iter()
            .map(CollectionVersion::display)
            .collect::<serde_json::Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string_pretty(&display)?);
        Ok(())
    }
}
