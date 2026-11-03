//! Lens migration command implementation

use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::document::write_client;
use super::{get_data_from_args, ClientContext};
use crate::error::{Error, Result};

/// Interact with Lens schema migrations
#[derive(Args, Debug)]
pub struct LensArgs {
    #[command(subcommand)]
    pub command: LensCommand,
}

/// Lens subcommands
#[derive(Subcommand, Debug)]
#[non_exhaustive]
pub enum LensCommand {
    /// Add a lens migration
    Add(LensAddArgs),
    /// List lens migrations
    List(LensListArgs),
    /// Set a migration between schema versions
    Set(LensSetArgs),
}

/// Arguments for lens add command
#[derive(Args, Debug)]
pub struct LensAddArgs {
    /// The lens configuration (JSON format)
    #[arg(value_name = "CONFIG")]
    pub config: Option<String>,

    /// Read lens configuration from file
    #[arg(long, short = 'f', value_name = "FILE")]
    pub file: Option<PathBuf>,
}

/// Arguments for lens list command
#[derive(Args, Debug)]
pub struct LensListArgs {}

/// Arguments for lens set command
#[derive(Args, Debug)]
pub struct LensSetArgs {
    /// Source collection version ID
    #[arg(value_name = "SRC")]
    pub src: Option<String>,

    /// Destination collection version ID
    #[arg(value_name = "DST")]
    pub dst: Option<String>,

    /// The lens configuration (JSON format)
    #[arg(value_name = "CONFIG")]
    pub config: Option<String>,

    /// Read lens configuration from file
    #[arg(long, short = 'f', value_name = "FILE")]
    pub file: Option<PathBuf>,
}

impl LensArgs {
    /// Execute the lens command
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        match &self.command {
            LensCommand::Add(args) => args.execute(ctx).await,
            LensCommand::List(args) => args.execute(ctx).await,
            LensCommand::Set(args) => args.execute(ctx).await,
        }
    }
}

impl LensAddArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let lens: serde_json::Value =
            serde_json::from_str(&get_data_from_args(&self.config, &self.file)?)?;
        let (client, txn) = write_client(ctx)?;
        let lens_id = client.add_lens(txn.as_deref(), &lens).await?;
        print_lens_id(&lens_id)
    }
}

impl LensListArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let (client, txn) = write_client(ctx)?;
        let lenses = client.list_lenses(txn.as_deref()).await?;
        println!("{}", serde_json::to_string_pretty(&lenses)?);
        Ok(())
    }
}

impl LensSetArgs {
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let src = self.src.as_ref().ok_or_else(|| {
            Error::MissingInput("source collection version ID (SRC) is required".into())
        })?;
        let dst = self.dst.as_ref().ok_or_else(|| {
            Error::MissingInput("destination collection version ID (DST) is required".into())
        })?;
        let lens: serde_json::Value =
            serde_json::from_str(&get_data_from_args(&self.config, &self.file)?)?;
        let config = serde_json::json!({
            "SourceCollectionVersionID": src,
            "DestinationCollectionVersionID": dst,
            "Lenses": lens.get("Lenses").cloned().unwrap_or_else(|| serde_json::json!([])),
        });
        let (client, txn) = write_client(ctx)?;
        let lens_id = client.set_migration(txn.as_deref(), &config).await?;
        print_lens_id(&lens_id)
    }
}

fn print_lens_id(lens_id: &str) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "lensId": lens_id }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lens_set_args_with_positional() {
        let args = LensSetArgs {
            src: Some("v1".to_string()),
            dst: Some("v2".to_string()),
            config: Some(r#"{"Lenses": []}"#.to_string()),
            file: None,
        };
        assert!(args.src.is_some());
        assert!(args.dst.is_some());
        assert!(args.config.is_some());
    }

    #[test]
    fn test_lens_set_args_with_file() {
        let args = LensSetArgs {
            src: None,
            dst: None,
            config: None,
            file: Some(PathBuf::from("migration.json")),
        };
        assert!(args.file.is_some());
    }

    #[test]
    fn test_lens_add_args_with_config() {
        let args = LensAddArgs {
            config: Some(r#"{"module": "test"}"#.to_string()),
            file: None,
        };
        assert!(args.config.is_some());
    }
}
