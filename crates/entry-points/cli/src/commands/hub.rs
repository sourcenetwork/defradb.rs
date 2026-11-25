//! `defradb hub`: a pinc hub, which the topics and the KMS key exchange of the
//! nodes naming it in `net.pinc_hubs` run through.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Args;
use tracing::info;

use crate::config::Config;
use crate::error::{Error, Result};

/// Arguments for the hub command.
#[derive(Args, Debug)]
pub struct HubArgs {
    /// The UDP address the hub binds. The address it prints names this one,
    /// so nodes reach it only at an address they can route to.
    #[arg(long, default_value = "127.0.0.1:9172")]
    pub bind: SocketAddr,

    /// The file holding the hub's 32-byte key, so its peer id survives a
    /// restart. A missing file gets a new key (default: `<rootdir>/hub.key`).
    #[arg(long)]
    pub key_file: Option<PathBuf>,
}

impl HubArgs {
    /// Run the hub until SIGINT or SIGTERM, printing the address nodes name
    /// it by in `net.pinc_hubs`.
    pub async fn execute(&self, config: Config, edge: edge::Edge) -> Result<()> {
        #[cfg(unix)]
        let (mut sigint, mut sigterm) = {
            use nuclei::signal::unix::{signal, SignalKind};
            (
                signal(SignalKind::interrupt()).map_err(|e| Error::Signal(e.to_string()))?,
                signal(SignalKind::terminate()).map_err(|e| Error::Signal(e.to_string()))?,
            )
        };
        let key_file = self
            .key_file
            .clone()
            .unwrap_or_else(|| config.rootdir.join("hub.key"));
        let seed = defra_pinc::load_or_generate_seed(Some(&key_file)).await?;
        let (address, hub) = defra_pinc::spawn_hub(*seed, self.bind, edge).await?;
        println!("{address}");
        info!(%address, "pinc hub running");

        #[cfg(unix)]
        nuclei::select! {
            _ = sigint.recv() => info!("Received SIGINT"),
            _ = sigterm.recv() => info!("Received SIGTERM"),
        }
        #[cfg(not(unix))]
        nuclei::signal::ctrl_c()
            .await
            .map_err(|e| Error::Signal(e.to_string()))?;

        drop(hub);
        info!("pinc hub stopped");
        Ok(())
    }
}
