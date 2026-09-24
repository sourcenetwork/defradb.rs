use clap::{Args, Subcommand};

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
}

impl StorageArgs {
    pub async fn execute(&self, config: &Config) -> Result<()> {
        if config.datastore.store == DatastoreType::Memory {
            return Err(Error::InvalidDatastore(
                "storage stats requires an on-disk database".into(),
            ));
        }
        if config.datastore.at_rest_encryption {
            return Err(Error::InvalidDatastore(
                "use the live database storage_stats API for an encrypted store".into(),
            ));
        }
        let StorageCommand::Stats { versions } = self.command;
        let txn = storage::backends::RegolithTxn::open_read_only(config.data_path())?;
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
}
