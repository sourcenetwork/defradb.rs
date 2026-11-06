//! An embedder's stream protocol rides the node's own connections, is
//! admitted as Defra's protocols are, and its handler ends at its deadline, on
//! a revocation and at shutdown without holding the peer's budget meanwhile.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use iroh::SecretKey;
use regolith::sync::{UnboundedReceiver, UnboundedSender};

use super::allowlist_tests::{config, shutdown_all};
use super::endpoint_config::AdmissionAuthority;
use super::inbound_budget::peer_share_bytes;
use super::{
    spawn_endpoint_with_protocols, InboundStream, IrohAllowlistConfig, IrohTransport, Priority,
    ProtocolRecvStream, ProtocolSendStream, StreamHandler, StreamProtocol, RESET_CANCELLED,
};
use crate::error::{Error, Result};
use crate::rt::time::timeout;
use crate::transport::{P2PTransport, PeerId};

const ECHO: StreamProtocol = StreamProtocol {
    tag: b"/test/echo/1",
    priority: Priority::Sync,
    max_message: 1024,
    deadline: Duration::from_secs(60),
};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
enum Behaviour {
    /// Replies with the request reversed.
    Echo,
    /// Never returns.
    Stuck,
}

/// What a probe handler reports: the peer of each stream it starts, and each
/// stream whose handler was dropped.
struct Signals {
    started: UnboundedReceiver<PeerId>,
    dropped: UnboundedReceiver<()>,
}

impl Signals {
    async fn started(&self) -> PeerId {
        timeout(IO_TIMEOUT, self.started.recv_async())
            .await
            .expect("the handler started")
            .unwrap()
    }

    async fn dropped(&self) {
        timeout(IO_TIMEOUT, self.dropped.recv_async())
            .await
            .expect("the handler was dropped")
            .unwrap();
    }
}

struct Probe {
    behaviour: Behaviour,
    started: UnboundedSender<PeerId>,
    dropped: UnboundedSender<()>,
}

struct OnDrop(UnboundedSender<()>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

#[async_trait]
impl StreamHandler for Probe {
    async fn handle(&self, stream: InboundStream) {
        let _ = self.started.try_send(stream.peer.clone());
        let _dropped = OnDrop(self.dropped.clone());
        match self.behaviour {
            Behaviour::Echo => {
                let InboundStream {
                    request, mut send, ..
                } = stream;
                let reply: Vec<u8> = request.iter().rev().copied().collect();
                let _ = send.write_all(&reply).await;
                let _ = send.finish();
            }
            Behaviour::Stuck => {
                let _held = stream;
                std::future::pending::<()>().await;
            }
        }
    }
}

fn probe(behaviour: Behaviour) -> (Arc<dyn StreamHandler>, Signals) {
    let (started_tx, started) = regolith::sync::unbounded();
    let (dropped_tx, dropped) = regolith::sync::unbounded();
    let handler = Arc::new(Probe {
        behaviour,
        started: started_tx,
        dropped: dropped_tx,
    });
    (handler, Signals { started, dropped })
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

/// Open `protocol` to `server`, dialling it at its listen addresses if no
/// connection is open.
async fn open(
    client: &IrohTransport,
    server: &IrohTransport,
    protocol: &StreamProtocol,
    request: Vec<u8>,
) -> Result<(ProtocolSendStream, ProtocolRecvStream)> {
    let addrs = server.listen_addresses().await?;
    timeout(
        IO_TIMEOUT,
        client.open_stream(server.local_peer_id(), addrs, protocol, request),
    )
    .await
    .map_err(|_| Error::ResponseTimeout)?
}

/// Send `ping` on an `ECHO` stream and read the whole reply.
async fn ping(client: &IrohTransport, server: &IrohTransport) -> Result<Vec<u8>> {
    let (send, mut recv) = open(client, server, &ECHO, b"ping".to_vec()).await?;
    send.finish()?;
    timeout(IO_TIMEOUT, recv.read_to_end(ECHO.max_message))
        .await
        .map_err(|_| Error::ResponseTimeout)?
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.round_trip")]
async fn a_request_and_its_reply_round_trip_without_dialling_first() {
    let (echo, signals) = probe(Behaviour::Echo);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, echo)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    assert_eq!(&signals.started().await, client.local_peer_id());

    shutdown_all(client, server, client_task, server_task).await;
}

/// The connection `open_stream` dials serves the streams the other side
/// opens back on it: here, the server's identity request.
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.two_way")]
async fn the_connection_a_stream_dials_accepts_the_peers_streams() {
    let (echo, _signals) = probe(Behaviour::Echo);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, echo)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    let identity = timeout(IO_TIMEOUT, server.get_peer_identity(client.local_peer_id()))
        .await
        .expect("the client answered the server's stream");
    assert!(matches!(identity, Ok(None)), "{identity:?}");

    shutdown_all(client, server, client_task, server_task).await;
}

/// A stream with a tag nobody registered is stopped before its request is
/// read, with the unknown-service code, and the connection keeps serving the
/// registered tag.
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.admission")]
async fn an_unregistered_tag_is_still_refused() {
    let (echo, signals) = probe(Behaviour::Echo);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, echo)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let unregistered = StreamProtocol {
        tag: b"/test/unregistered/1",
        ..ECHO
    };
    let (mut send, _recv) = open(&client, &server, &unregistered, b"ping".to_vec())
        .await
        .unwrap();
    let refused = timeout(IO_TIMEOUT, async {
        loop {
            if let Err(error) = send.write_all(b"more").await {
                return error;
            }
            crate::rt::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the stream is stopped");
    assert!(matches!(refused, Error::StreamReset(2)), "{refused:?}");

    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    assert_eq!(&signals.started().await, client.local_peer_id());
    assert!(
        signals.started.try_recv().is_none(),
        "only the ping was served"
    );

    shutdown_all(client, server, client_task, server_task).await;
}

/// A peer the inbound allowlist refuses never reaches the handler. Admitting
/// it lets the same request through, so the allowlist is what refused it.
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.admission")]
async fn a_peer_the_allowlist_refuses_cannot_open_the_protocol() {
    let (echo, signals) = probe(Behaviour::Echo);
    let (server, server_task) = spawn_node(
        IrohAllowlistConfig::Explicit(Default::default()),
        vec![(ECHO, echo)],
    )
    .await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    assert!(ping(&client, &server).await.is_err());
    assert!(
        signals.started.try_recv().is_none(),
        "a refused peer reached the handler"
    );

    server
        .allow_peer(client.local_peer_id(), AdmissionAuthority::full())
        .await
        .unwrap();
    client.disconnect(server.local_peer_id()).await.unwrap();
    assert_eq!(ping(&client, &server).await.unwrap(), b"gnip");
    assert_eq!(&signals.started().await, client.local_peer_id());

    shutdown_all(client, server, client_task, server_task).await;
}

/// Two requests each as large as the peer's whole share of the priority are
/// both handed to a handler that never returns: the first gave its room back
/// once admitted, so nothing is held while the handlers run.
#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.budget")]
async fn a_stuck_handler_holds_none_of_the_peers_budget() {
    let share = peer_share_bytes(Priority::Control);
    let large = StreamProtocol {
        tag: b"/test/large/1",
        priority: Priority::Control,
        max_message: share,
        deadline: Duration::from_secs(60),
    };
    let (stuck, signals) = probe(Behaviour::Stuck);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(large, stuck)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let _first = open(&client, &server, &large, vec![1; share])
        .await
        .unwrap();
    signals.started().await;
    let _second = open(&client, &server, &large, vec![2; share])
        .await
        .unwrap();
    signals.started().await;

    let control = server
        .inbound_budget()
        .await
        .unwrap()
        .into_iter()
        .find(|class| class.class == Priority::Control.name())
        .unwrap();
    assert_eq!(control.held_bytes, 0);
    assert_eq!(control.busy_resets, 0);

    shutdown_all(client, server, client_task, server_task).await;
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.deadline")]
async fn a_handler_past_its_deadline_is_dropped_and_its_stream_reset() {
    let short = StreamProtocol {
        deadline: Duration::from_millis(300),
        ..ECHO
    };
    let (stuck, signals) = probe(Behaviour::Stuck);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(short, stuck)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let (_send, mut recv) = open(&client, &server, &short, b"ping".to_vec())
        .await
        .unwrap();
    signals.started().await;
    let read = timeout(IO_TIMEOUT, recv.read_to_end(1024))
        .await
        .expect("the stream ends");
    assert!(
        matches!(read, Err(Error::StreamReset(code)) if code == u64::from(RESET_CANCELLED)),
        "{read:?}"
    );
    signals.dropped().await;

    shutdown_all(client, server, client_task, server_task).await;
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.revocation")]
async fn denying_a_peer_cuts_its_open_protocol_streams() {
    let (stuck, signals) = probe(Behaviour::Stuck);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, stuck)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let (_send, mut recv) = open(&client, &server, &ECHO, b"ping".to_vec())
        .await
        .unwrap();
    signals.started().await;
    server.deny_peer(client.local_peer_id()).await.unwrap();
    signals.dropped().await;
    let read = timeout(IO_TIMEOUT, recv.read_to_end(1024))
        .await
        .expect("the stream ends");
    assert!(read.is_err(), "{read:?}");

    shutdown_all(client, server, client_task, server_task).await;
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.shutdown")]
async fn shutdown_cancels_a_running_handler() {
    let (stuck, signals) = probe(Behaviour::Stuck);
    let (server, server_task) =
        spawn_node(IrohAllowlistConfig::AcceptAll, vec![(ECHO, stuck)]).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let (_send, _recv) = open(&client, &server, &ECHO, b"ping".to_vec())
        .await
        .unwrap();
    signals.started().await;
    server.shutdown().await.unwrap();
    signals.dropped().await;

    client.shutdown().await.unwrap();
    client_task.await.unwrap();
    server_task.await.unwrap();
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.registration")]
async fn a_node_refuses_to_start_on_an_invalid_or_repeated_protocol() {
    let (echo, _signals) = probe(Behaviour::Echo);
    let reserved = StreamProtocol {
        tag: b"/defra-iroh/echo/1",
        ..ECHO
    };
    for protocols in [
        vec![(reserved, Arc::clone(&echo))],
        vec![(ECHO, Arc::clone(&echo)), (ECHO, Arc::clone(&echo))],
    ] {
        let started = spawn_endpoint_with_protocols(
            config(SecretKey::generate(), IrohAllowlistConfig::AcceptAll),
            edge::testing::edge(),
            protocols,
        )
        .await;
        assert!(matches!(started, Err(Error::InvalidConfig(_))));
    }
}

#[nuclei::test(threads = 0)]
#[mantra_macros::req_test("defra.stream_protocols.registration")]
async fn opening_a_reserved_tag_or_an_oversized_request_fails_locally() {
    let (server, server_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;
    let (client, client_task) = spawn_node(IrohAllowlistConfig::AcceptAll, Vec::new()).await;

    let reserved = StreamProtocol {
        tag: b"/defra-iroh/identity/0.1",
        ..ECHO
    };
    let opened = open(&client, &server, &reserved, Vec::new()).await;
    assert!(matches!(opened, Err(Error::InvalidConfig(_))));
    let oversized = open(&client, &server, &ECHO, vec![0; ECHO.max_message + 1]).await;
    assert!(matches!(oversized, Err(Error::InvalidConfig(_))));

    shutdown_all(client, server, client_task, server_task).await;
}
