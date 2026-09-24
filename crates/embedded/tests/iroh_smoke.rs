#![cfg(feature = "iroh")]

use std::net::{IpAddr, Ipv4Addr};

use anyhow::{bail, Context, Result};
use embedded::{IrohConfig, NodeBuilder};
use tokio::time::{sleep, Duration, Instant};

const BOOK_SDL: &str = "type Book { title: String }";
const REPLICATED_TITLE: &str = "Replicated over iroh";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_embedded_iroh_nodes_connect_and_replicate() -> Result<()> {
    let node_a = NodeBuilder::default()
        .with_iroh(test_iroh_config())
        .build()
        .await?;
    let node_b = NodeBuilder::default()
        .with_iroh(test_iroh_config())
        .build()
        .await?;

    node_a.add_schema(BOOK_SDL).await?;
    node_b.add_schema(BOOK_SDL).await?;

    let p2p_b = node_b.p2p().cloned().context("node_b missing p2p system")?;
    let p2p_a = node_a.p2p().cloned().context("node_a missing p2p system")?;

    let peer_a = p2p_a
        .ops()
        .local_peer_id()
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    let connect_addr_a = wait_for_connectable_iroh_addr(&p2p_a).await?;

    let create_response = node_a
        .execute(
            r#"mutation { add_Book(input: {title: "Replicated over iroh"}) { _docID title } }"#,
        )
        .await;
    ensure_success(&create_response, "add_Book")?;
    let doc_id = extract_created_doc_id(&create_response)?;

    p2p_b
        .ops()
        .connect_peer(&connect_addr_a)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    wait_for_connected_peer(&p2p_b, &peer_a).await?;

    p2p_b
        .ops()
        .sync_documents("Book", vec![doc_id], None)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;

    wait_for_book_title(&node_b, REPLICATED_TITLE).await?;

    p2p_a.shutdown().await;
    p2p_b.shutdown().await;
    node_a.database.close().await?;
    node_b.database.close().await?;

    Ok(())
}

/// A node built with an explicit allowlist admits the peer it is told to
/// admit, and that peer's inbound connection lands.
///
/// The allowlist is only reachable from an embedder through
/// [`IrohConfig::allowlist`], and admitting a peer is a no-op under
/// `AcceptAll`, so a node that could not be built with `Explicit` could not
/// use the feature at all. Starting from an empty explicit set is the
/// strictest case: nothing is authorized yet, so the connection that
/// succeeds below succeeds because of the authorization and nothing else.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_explicit_allowlist_admits_the_peer_it_is_given() -> Result<()> {
    let listener = NodeBuilder::default()
        .with_iroh(IrohConfig {
            allowlist: p2p::iroh::IrohAllowlistConfig::Explicit(Default::default()),
            ..test_iroh_config()
        })
        .build()
        .await?;
    let dialer = NodeBuilder::default()
        .with_iroh(test_iroh_config())
        .build()
        .await?;

    let listener_p2p = listener
        .p2p()
        .cloned()
        .context("listener missing p2p system")?;
    let dialer_p2p = dialer.p2p().cloned().context("dialer missing p2p system")?;

    let dialer_peer = dialer_p2p
        .ops()
        .local_peer_id()
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    let listener_addr = wait_for_connectable_iroh_addr(&listener_p2p).await?;

    // Authorize the other node, by its own endpoint id, before it dials.
    let admitted =
        defra_http::TransportPeerId::new(dialer_peer.clone()).map_err(|e| anyhow::anyhow!(e))?;
    listener_p2p
        .ops()
        .allow_peer(&admitted)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;

    dialer_p2p
        .ops()
        .connect_peer(&listener_addr)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;

    // The inbound side is what the allowlist governs, so the assertion is
    // that the listener sees the peer, not merely that the dial returned.
    wait_for_connected_peer(&listener_p2p, &dialer_peer).await?;

    listener_p2p.shutdown().await;
    dialer_p2p.shutdown().await;
    listener.database.close().await?;
    dialer.database.close().await?;
    Ok(())
}

fn test_iroh_config() -> IrohConfig {
    IrohConfig {
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        bind_port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
        allowlist: Default::default(),
    }
}

async fn wait_for_connectable_iroh_addr(system: &embedded::ManagedP2PSystem) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let addrs = system
            .ops()
            .listen_addresses()
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        if let Some(addr) = addrs
            .into_iter()
            .find(|addr| addr.contains("/p2p/") || addr.starts_with("endpoint"))
        {
            return Ok(addr);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for direct iroh listen address");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_connected_peer(system: &embedded::ManagedP2PSystem, peer_id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let peers = system
            .ops()
            .connected_peers()
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        if peers.iter().any(|peer| {
            p2p::iroh::parse_public_peer_addr(peer)
                .map(|(parsed_peer_id, _)| parsed_peer_id.as_str() == peer_id)
                .unwrap_or_else(|_| peer.contains(peer_id))
        }) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for peer connection to {peer_id}");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_book_title(
    node: &embedded::EmbeddedNode<embedded::EmbeddedStore>,
    title: &str,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let response = node.execute("query { Book { _docID title } }").await;
        ensure_success(&response, "Book query")?;

        if response_contains_title(&response, title) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for replicated title '{title}'");
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn ensure_success(response: &query::QueryResponse, operation: &str) -> Result<()> {
    if response.has_errors() {
        bail!("{operation} returned errors: {:?}", response.errors);
    }
    Ok(())
}

fn response_contains_title(response: &query::QueryResponse, title: &str) -> bool {
    response
        .data
        .as_ref()
        .and_then(|data| data.get("Book"))
        .and_then(|books| books.as_array())
        .map(|books| {
            books
                .iter()
                .any(|book| book.get("title").and_then(|value| value.as_str()) == Some(title))
        })
        .unwrap_or(false)
}

fn extract_created_doc_id(response: &query::QueryResponse) -> Result<String> {
    response
        .data
        .as_ref()
        .and_then(|data| data.get("add_Book"))
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("_docID"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("add_Book response missing _docID"))
}
