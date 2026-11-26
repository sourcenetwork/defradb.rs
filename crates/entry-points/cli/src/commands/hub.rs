//! `defradb hub`: a pinc hub, which the topics and the KMS key exchange of the
//! nodes naming it in `net.pinc_hubs` run through, and with `--relay` the
//! sessions of nodes that cannot reach each other directly.

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
    /// so nodes reach it only at an address they can route to. A wildcard
    /// (`0.0.0.0:9172`) prints every interface's address instead.
    #[arg(long, default_value = "127.0.0.1:9172")]
    pub bind: SocketAddr,

    /// An address nodes reach the hub at that is on none of its interfaces
    /// (a public address in front of a NAT), printed first. Repeatable.
    #[arg(long)]
    pub public_addr: Vec<SocketAddr>,

    /// Also run pinc's relay role: nodes naming the hub that cannot reach
    /// each other directly (behind NATs) reach each other through it, by
    /// peer id. Off by default: the hub then carries topics only.
    #[arg(long)]
    pub relay: bool,

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
        let mut hub_config = defra_pinc::HubConfig::new(seed, self.bind);
        hub_config.public_addrs = self.public_addr.clone();
        hub_config.relay = self.relay;
        let (address, hub) = defra_pinc::spawn_hub(hub_config, edge).await?;
        println!("{address}");
        info!(%address, relay = self.relay, "pinc hub running");

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
