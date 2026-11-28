use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use storage::corekv::Store;

use crate::{
    config::{Config, DatastoreType},
    error::{Error, Result},
};

#[derive(Args, Debug)]
pub struct StorageArgs {
    #[command(subcommand)]
    command: StorageCommand,
}

#[derive(Subcommand, Debug)]
enum StorageCommand {
    /// Report logical byte counts as JSON without starting or modifying the database.
    Stats {
        /// Count versions per document and field (uses memory proportional to document count).
        #[arg(long)]
        versions: bool,
    },
    /// Copy a stopped Regolith database into a new directory on the same filesystem.
    /// Opening the source may upgrade it or reseal its manifest under the current key.
    Checkpoint { destination: PathBuf },
}

impl StorageArgs {
    pub async fn execute(&self, config: &Config) -> Result<()> {
        match &self.command {
            StorageCommand::Stats { versions } => Self::stats(config, *versions).await,
            StorageCommand::Checkpoint { destination } => {
                Self::checkpoint(config, destination).await
            }
        }
    }

    async fn stats(config: &Config, versions: bool) -> Result<()> {
        if config.datastore.store == DatastoreType::Memory {
            return Err(Error::InvalidDatastore(
                "storage stats requires an on-disk database".into(),
            ));
        }
        let txn = storage::backends::RegolithTxn::open_read_only(
            config.data_path(),
            crate::commands::start::Node::encryption_options(
                config,
                storage::RegolithStoreOptions::default(),
                false,
            )?,
        )?;
        let stats = db::database::storage_stats::collect(&txn, versions)
            .await
            .map_err(|error| Error::Server(error.to_string()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&stats)
                .map_err(|error| Error::Server(error.to_string()))?
        );
        Ok(())
    }

    async fn checkpoint(config: &Config, destination: &Path) -> Result<()> {
        if config.datastore.store == DatastoreType::Memory {
            return Err(Error::InvalidDatastore(
                "storage checkpoint requires an on-disk database".into(),
            ));
        }
        let source = config.data_path();
        match std::fs::metadata(source.join("MANIFEST")) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::InvalidDatastore(format!(
                    "storage checkpoint source has no manifest: {}",
                    source.display()
                )));
            }
            Err(error) => return Err(error.into()),
        }
        let options = crate::commands::start::Node::encryption_options(
            config,
            storage::RegolithStoreOptions::default(),
            false,
        )?;
        let store = storage::RegolithStore::open_with_options(source, options)?;
        let checkpoint = store.checkpoint_to_new_directory(destination).await;
        let close = store.close().await;
        checkpoint?;
        close?;
        Ok(())
    }
}
