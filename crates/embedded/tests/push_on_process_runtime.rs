#![cfg(feature = "iroh")]

//! A node built without a network runtime pushes to a peer that connects
//! after the push began. The push runs on a pool worker, which has no
//! reactor, and its wait for the peer sleeps there unless it crosses to the
//! runtime the node's database was given: the process's own, which the
//! node's transport already runs on.

#[path = "support/clock.rs"]
mod clock;
mod iroh_peers;
#[path = "support/outcome.rs"]
mod outcome;

use std::sync::Arc;
use std::time::Duration;

use clock::sleep;
use defra_http::router::P2pDocumentRequest;
use embedded::NodeBuilder;
use iroh_peers::{connect, create, iroh_config, listen_addr, Node};
use outcome::Result;

const SDL: &str = "type Note { body: String }";

async fn node() -> Result<Node> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .build()
        .await?;
    node.add_schema(SDL).await?;
    Ok(node)
}

#[test]
fn a_push_from_a_node_without_a_network_runtime_waits_for_its_peer() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let pusher = node().await?;
            let receiver = node().await?;
            receiver
                .p2p()
                .expect("p2p")
                .ops()
                .add_collections(vec!["Note".to_string()])
                .await?;
            let (doc_id, _) = create(&pusher, "Note", r#"body: "pushed""#).await?;
            let receiver_addr = listen_addr(&receiver).await?;

            // The push starts before the peers are connected, so its wait for
            // the peer sleeps at least once before the connection lands.
            let ops = Arc::clone(pusher.p2p().expect("p2p").ops());
            let push = edge::testing::edge().spawn(async move {
                ops.push_documents_to_peer(
                    &receiver_addr,
                    vec![P2pDocumentRequest {
                        collection: "Note".to_string(),
                        doc_id,
                    }],
                )
                .await
            });
            sleep(Duration::from_millis(500)).await;
            assert!(!push.is_finished(), "the push did not wait for its peer");
            connect(&pusher, &receiver).await?;

            push.await.expect("the push task ran")?;

            pusher.shutdown().await;
            receiver.shutdown().await;
            Ok(())
        })
        .unwrap()
}
