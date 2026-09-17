//! Iroh embedded nodes on localhost, and the GraphQL calls peer tests share.
//!
//! Each test binary including this module uses its own subset of the helpers.
#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr};

use anyhow::{anyhow, bail, Context, Result};
use embedded::{EmbeddedNode, EmbeddedStore, IrohConfig};
use serde_json::Value as JsonValue;
use tokio::time::{sleep, Duration, Instant};

pub type Node = EmbeddedNode<EmbeddedStore>;

pub fn iroh_config() -> IrohConfig {
    IrohConfig {
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        bind_port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        ..Default::default()
    }
}

/// Create a document and return its ID and its genesis composite's CID.
pub async fn create(node: &Node, collection: &str, input: &str) -> Result<(String, String)> {
    let response = node
        .execute(&format!(
            "mutation {{ add_{collection}(input: {{{input}}}) {{ _docID _version {{ cid }} }} }}"
        ))
        .await;
    if response.has_errors() {
        bail!("add_{collection} failed: {:?}", response.errors);
    }
    let created = response
        .data
        .as_ref()
        .and_then(|data| data.get(format!("add_{collection}")))
        .and_then(JsonValue::as_array)
        .and_then(|items| items.first())
        .context("created document")?;
    let doc_id = created
        .get("_docID")
        .and_then(JsonValue::as_str)
        .context("_docID")?;
    let cid = created
        .get("_version")
        .and_then(JsonValue::as_array)
        .and_then(|versions| versions.first())
        .and_then(|version| version.get("cid"))
        .and_then(JsonValue::as_str)
        .context("_version cid")?;
    Ok((doc_id.to_string(), cid.to_string()))
}

/// The address other nodes connect to `node` on.
pub async fn listen_addr(node: &Node) -> Result<String> {
    let ops = node.p2p().context("p2p")?.ops();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let addrs = ops
            .listen_addresses()
            .await
            .map_err(|error| anyhow!(error))?;
        if let Some(addr) = addrs
            .into_iter()
            .find(|addr| addr.contains("/p2p/") || addr.starts_with("endpoint"))
        {
            return Ok(addr);
        }
        if Instant::now() >= deadline {
            bail!("no iroh listen address");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

/// Connect `from` to `to` and return `to`'s peer ID.
pub async fn connect(from: &Node, to: &Node) -> Result<String> {
    let from = from.p2p().context("p2p")?;
    let peer = to
        .p2p()
        .context("p2p")?
        .ops()
        .local_peer_id()
        .await
        .map_err(|error| anyhow!(error))?;
    let addr = listen_addr(to).await?;
    let deadline = Instant::now() + Duration::from_secs(10);
    from.ops()
        .connect_peer(&addr)
        .await
        .map_err(|error| anyhow!(error))?;
    loop {
        let peers = from
            .ops()
            .connected_peers()
            .await
            .map_err(|error| anyhow!(error))?;
        if peers.iter().any(|connected| connected.contains(&peer)) {
            return Ok(peer);
        }
        if Instant::now() >= deadline {
            bail!("peer {peer} never connected");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

pub async fn sync(node: &Node, collection: &str, doc_ids: Vec<String>) -> Result<()> {
    node.p2p()
        .context("p2p")?
        .ops()
        .sync_documents(collection, doc_ids, None)
        .await
        .map_err(|error| anyhow!(error))
}

pub async fn doc_ids(node: &Node, collection: &str) -> Result<Vec<String>> {
    let response = node
        .execute(&format!("query {{ {collection} {{ _docID }} }}"))
        .await;
    if response.has_errors() {
        bail!("{collection} query failed: {:?}", response.errors);
    }
    let mut ids: Vec<String> = response
        .data
        .as_ref()
        .and_then(|data| data.get(collection))
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|note| note.get("_docID").and_then(JsonValue::as_str))
        .map(str::to_string)
        .collect();
    ids.sort();
    Ok(ids)
}

/// Subscribe `to` to `collections` and give `from` a replicator pushing them
/// to it.
pub async fn replicate(from: &Node, to: &Node, collections: &[&str]) -> Result<()> {
    let collections: Vec<String> = collections.iter().map(|name| name.to_string()).collect();
    to.p2p()
        .context("p2p")?
        .ops()
        .add_collections(collections.clone())
        .await
        .map_err(|error| anyhow!(error))?;
    let addr = listen_addr(to).await?;
    from.p2p()
        .context("p2p")?
        .ops()
        .add_replicator(
            collections,
            Some(&addr),
            Default::default(),
            Vec::new(),
            None,
        )
        .await
        .map_err(|error| anyhow!(error))
}

pub async fn wait_for_docs(node: &Node, collection: &str, expected: &[&str]) -> Result<()> {
    let mut expected: Vec<String> = expected.iter().map(|id| id.to_string()).collect();
    expected.sort();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ids = doc_ids(node, collection).await?;
        if ids == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("{collection} {ids:?}, expected {expected:?}");
        }
        sleep(Duration::from_millis(200)).await;
    }
}
