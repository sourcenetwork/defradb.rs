//! Collection operation command implementations

use super::{CollectionDeleteArgs, CollectionPatchArgs, SetActiveArgs, TruncateArgs};
use crate::commands::client::document::write_client;
use crate::commands::client::http_client::HttpClient;
use crate::commands::client::ClientContext;
use crate::commands::client::{get_data_from_args, validate_identifier};
use crate::error::{Error, Result};

impl CollectionPatchArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let patch = get_data_from_args(&self.patch, &self.patch_file)?;
        let migration = match self.migration.is_some() || self.lens_file.is_some() {
            true => Some(serde_json::from_str::<serde_json::Value>(
                &get_data_from_args(&self.migration, &self.lens_file)?,
            )?),
            false => None,
        };
        let (client, txn) = write_client(ctx)?;
        client
            .patch_collection(txn.as_deref(), &patch, migration.as_ref())
            .await
    }
}

impl SetActiveArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let (client, txn) = write_client(ctx)?;
        client
            .set_active_collection_version(txn.as_deref(), self.version_id.as_deref().unwrap_or(""))
            .await
    }
}

impl TruncateArgs {
    pub async fn execute(&self, ctx: &ClientContext, name: Option<&str>) -> Result<()> {
        let collection =
            name.ok_or_else(|| Error::MissingInput("--name is required for truncate".to_string()))?;
        validate_identifier(collection)?;

        let client = HttpClient::new(&ctx.url)?
            .with_auth_token(ctx.auth_token.clone())
            .with_verbose(ctx.verbose);

        let filter = self
            .filter
            .as_deref()
            .map(parse_truncate_filter)
            .transpose()?;

        client
            .collection_truncate(collection, filter.as_ref())
            .await?;
        println!("Truncated collection {}", collection);
        Ok(())
    }
}

fn parse_truncate_filter(value: &str) -> Result<serde_json::Value> {
    let filter = serde_json::from_str::<serde_json::Value>(value)?;
    if !filter.is_object() {
        return Err(Error::MissingInput(
            "--filter must be a non-null JSON object".to_string(),
        ));
    }
    Ok(filter)
}

impl CollectionDeleteArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let names: Vec<String> = self
            .names
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if names.is_empty() {
            return Err(Error::MissingInput(
                "at least one collection name is required".to_string(),
            ));
        }

        for name in &names {
            validate_identifier(name)?;
        }

        let client = HttpClient::new(&ctx.url)?
            .with_auth_token(ctx.auth_token.clone())
            .with_verbose(ctx.verbose);

        client.collection_delete(&names, self.active_only).await?;

        if names.len() == 1 {
            println!("Deleted collection {}", names[0]);
        } else {
            println!("Deleted collections {}", names.join(", "));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_truncate_filter;

    #[test]
    fn truncate_filter_requires_a_json_object() {
        assert!(parse_truncate_filter(r#"{"name":{"_eq":"Alice"}}"#).is_ok());
        for invalid in ["null", "[]", "not json"] {
            assert!(parse_truncate_filter(invalid).is_err());
        }
    }
}
