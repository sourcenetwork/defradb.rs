//! An embedder's stream protocol rides the node's own connections and is
//! admitted as Defra's protocols are.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use iroh::{EndpointId, SecretKey};
use regolith::sync::{UnboundedReceiver, UnboundedSender};

use super::allowlist_tests::{config, dial_and_wait_for_local_handshake, shutdown_all};
use super::endpoint_config::AdmissionAuthority;
use super::{
    spawn_endpoint_with_protocols, InboundStream, IrohAllowlistConfig, IrohTransport, Priority,
    StreamHandler, StreamProtocol,
};
use crate::error::{Error, Result};
use crate::rt::time::timeout;
use crate::transport::P2PTransport;

const ECHO: StreamProtocol = StreamProtocol {
    tag: b"/test/echo/0.1",
    priority: Priority::Sync,
    max_message: 1024,
};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Replies with the request reversed, and reports the peer that asked.
struct Echo {
    peers: UnboundedSender<EndpointId>,
}

#[async_trait]
impl StreamHandler for Echo {
    async fn handle(&self, mut stream: InboundStream) {
        let _ = self.peers.try_send(stream.peer);
        let reply: Vec<u8> = stream.request.iter().rev().copied().collect();
        let _ = stream.send.write_all(&reply).await;
        let _ = stream.send.finish();
    }
}

fn echo() -> (Arc<dyn StreamHandler>, UnboundedReceiver<EndpointId>) {
    let (peers, served) = regolith::sync::unbounded();
    (Arc::new(Echo { peers }), served)
}

async fn spawn_node(
    allowlist: IrohAllowlistConfig,
    stream_protocols: Vec<(StreamProtocol, Arc<dyn StreamHandler>)>,
) -> (IrohTransport, edge::Task<()>) {
    let secret_key = SecretKey::generate();
    let (command_tx, _events, _replicators, task) = spawn_endpoint_with_protocols(
        config(secret_key.clone(), allowlist),
        edge::testing::edge(),
        stream_protocols,
    )
    .await
    .unwrap();
    (IrohTransport::new(command_tx, secret_key), task)
}

async fn dial(client: &IrohTransport, server: &IrohTransport) {
    client
        .dial(
            server.local_peer_id(),
            server.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
}

/// Send `ping` to `server` on an `ECHO` stream and read the whole reply.
async fn ping(client: &IrohTransport, server: &IrohTransport) -> Result<Vec<u8>> {
    let exchange = async {
        let (mut send, mut recv) = client
            .open_stream(server.local_peer_id(), &ECHO, b"ping".to_vec())
            .await?;
        send.finish()
            .map_err(|error| Error::Transport(error.to_string()))?;
        recv.read_to_end(ECHO.max_message)
            .await
            .map_err(|error| Error::Transport(error.to_string()))
    };
    timeout(IO_TIMEOUT, exchange)
        .await
        .map_err(|_| Error::ResponseTimeout)?
}

#[nuclei::test(threads = 0)]
async fn a_request_and_its_reply_round_trip_over_a_registered_tag() {
    let (echo, served) = echo();
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, echo)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;
    dial(&client, &server).await;

    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    let peer = served.try_recv().expect("the handler ran");
    assert_eq!(peer.to_string(), client.local_peer_id().as_str());

    shutdown_all(client, server, client_task, server_task).await;
}

/// A stream with a tag nobody registered is stopped before its request is
/// read, with the unknown-service code, and the connection keeps serving the
/// registered tag.
#[nuclei::test(threads = 0)]
async fn an_unregistered_tag_is_still_refused() {
    let (echo, served) = echo();
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, echo)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;
    dial(&client, &server).await;

    let unregistered = StreamProtocol {
        tag: b"/test/unregistered/0.1",
        ..ECHO
    };
    let (send, _recv) = client
        .open_stream(server.local_peer_id(), &unregistered, b"ping".to_vec())
        .await
        .unwrap();
    let code = timeout(IO_TIMEOUT, send.stopped())
        .await
        .expect("the stream is stopped")
        .unwrap()
        .map(|code| code.into_inner());
    assert_eq!(code, Some(2));
    assert!(served.try_recv().is_none());

    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");

    shutdown_all(client, server, client_task, server_task).await;
}

/// A peer the inbound allowlist refuses never reaches the handler. Admitting
/// it lets the same request through, so the allowlist is what refused it.
#[nuclei::test(threads = 0)]
async fn a_peer_the_allowlist_refuses_cannot_open_the_protocol() {
    let (echo, served) = echo();
    let (server, server_task) = spawn_node(
        IrohAllowlistConfig::Explicit(Default::default()),
        vec![(ECHO, echo)],
    )
    .await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    dial_and_wait_for_local_handshake(&client, &server).await;
    assert!(ping(&client, &server).await.is_err());
    assert!(
        served.try_recv().is_none(),
        "a refused peer reached the handler"
    );

    server
        .allow_peer(client.local_peer_id(), AdmissionAuthority::full())
        .await
        .unwrap();
    dial(&client, &server).await;
    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    assert!(served.try_recv().is_some());

    shutdown_all(client, server, client_task, server_task).await;
}
