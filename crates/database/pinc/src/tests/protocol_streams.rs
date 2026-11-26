//! Embedder stream protocols at their edges: an open refused busy leaves the
//! peer nothing to serve, and a handler blocked writing to an opener that
//! does not read is reset at its deadline.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::{connect, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::{
    InboundStream, PincEndpointConfig, Priority, StreamHandler, StreamProtocol, RESET_CANCELLED,
};
use p2p::rt::mpsc;
use p2p::transport::P2PTransport;

const STARTED: &[u8] = b"started";
/// Far more than pinc buffers for a session's streams (its 16 MiB pool),
/// the stream window and the receive queue together: a write this large
/// waits on the opener reading.
const FLOOD: usize = 40 * 1024 * 1024;
/// More than an opener that stopped reading can hold: its stream window
/// and its receive queue.
const IN_FLIGHT: usize = 4 * 1024 * 1024;

fn protocol(tag: &'static [u8], deadline: Duration) -> StreamProtocol {
    StreamProtocol {
        tag,
        priority: Priority::Sync,
        max_message: 1024,
        deadline,
    }
}

async fn serving(protocol: StreamProtocol, handler: Arc<dyn StreamHandler>) -> TestNode {
    let mut config = PincEndpointConfig::new(seed(), loopback());
    config.max_connections = 8;
    config.stream_protocols = vec![(protocol, handler)];
    TestNode::with_config(config).await
}

/// Counts the requests it serves and tells `served` of each.
struct Counting {
    count: AtomicUsize,
    served: mpsc::Sender<()>,
}

#[async_trait]
impl StreamHandler for Counting {
    async fn handle(&self, stream: &mut InboundStream) {
        self.count.fetch_add(1, Ordering::Relaxed);
        let _ = self.served.try_send(());
        if stream.send.write_all(b"served").await.is_ok() {
            let _ = stream.send.finish().await;
        }
    }
}

/// The opener's endpoint has room for its loops, the connection's serve
/// task and one more: the task that opens the stream fits, its pumps do not.
/// A request written before the pumps were placed would reach the peer's
/// handler while the opener hears `busy`, and a retry would run it twice.
#[test]
fn an_open_refused_busy_leaves_the_peer_no_request_to_serve() {
    run(async {
        let (served, mut heard) = mpsc::channel(16);
        let handler = Arc::new(Counting {
            count: AtomicUsize::new(0),
            served,
        });
        let tag: &'static [u8] = b"/test/count/1";
        let server = serving(protocol(tag, Duration::from_secs(5)), handler.clone()).await;
        let mut config = PincEndpointConfig::new(seed(), loopback());
        config.max_connections = 8;
        // Four loops, the connection's serve task, the opener.
        config.max_tasks = 6;
        let opener = TestNode::with_config(config).await;
        // The server dials, so the opener holds the link without a dial task.
        connect(&server.transport, &opener.transport).await;
        // Each attempt is refused busy; once the connection's short tasks
        // are gone, every one gets past its opener task to its pumps.
        for _ in 0..20 {
            let opened = opener
                .transport
                .open_stream(
                    &server.id,
                    Vec::new(),
                    &protocol(tag, Duration::from_secs(5)),
                    b"once".to_vec(),
                )
                .await;
            let error = opened
                .err()
                .expect("an open with no room for its pumps succeeded");
            assert!(error.to_string().contains("busy"), "{error}");
            p2p::rt::time::sleep(Duration::from_millis(20)).await;
        }
        let ran = p2p::rt::time::timeout(Duration::from_millis(500), heard.recv()).await;
        assert!(
            ran.is_err(),
            "a request whose open failed reached the handler"
        );
        assert_eq!(handler.count.load(Ordering::Relaxed), 0);
        opener.shutdown().await;
        server.shutdown().await;
    });
}

/// Writes `started`, then one write far larger than the opener takes
/// without reading, and never returns. Its guard tells `cut` when the node
/// drops it at its deadline.
struct Flood {
    cut: mpsc::Sender<()>,
}

struct CutGuard(mpsc::Sender<()>);

impl Drop for CutGuard {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

#[async_trait]
impl StreamHandler for Flood {
    async fn handle(&self, stream: &mut InboundStream) {
        let _guard = CutGuard(self.cut.clone());
        stream.request.release();
        if stream.send.write_all(STARTED).await.is_ok() {
            let _ = stream.send.write_all(&vec![1; FLOOD]).await;
        }
        std::future::pending::<()>().await;
    }
}

/// The handler's deadline passes while its write waits on an opener that
/// reads nothing: the stream is reset then, so the opener gets no more than
/// was in flight, rather than the whole write once it reads again.
#[test]
fn a_handler_blocked_writing_to_an_opener_that_does_not_read_is_reset_at_its_deadline() {
    run(async {
        let (cut, mut was_cut) = mpsc::channel(4);
        let tag: &'static [u8] = b"/test/flood/1";
        let deadline = Duration::from_millis(300);
        let server = serving(protocol(tag, deadline), Arc::new(Flood { cut })).await;
        let opener = TestNode::spawn().await;
        let addrs = server.transport.listen_addresses().await.unwrap();
        let (_send, mut recv) = opener
            .transport
            .open_stream(&server.id, addrs, &protocol(tag, deadline), b"go".to_vec())
            .await
            .unwrap();
        let mut started = vec![0; STARTED.len()];
        let mut filled = 0;
        while filled < started.len() {
            filled += recv.read(&mut started[filled..]).await.unwrap().unwrap();
        }
        assert_eq!(started, STARTED);
        p2p::rt::time::timeout(IO_TIMEOUT, was_cut.recv())
            .await
            .expect("the handler outlived its deadline");
        let mut buf = vec![0; 64 * 1024];
        let mut read = 0;
        let ended = p2p::rt::time::timeout(IO_TIMEOUT, async {
            loop {
                match recv.read(&mut buf).await {
                    Ok(Some(n)) => read += n,
                    Ok(None) => return None,
                    Err(error) => return Some(error),
                }
            }
        })
        .await
        .expect("the stream stayed open past its deadline");
        assert!(
            matches!(ended, Some(p2p::Error::StreamReset(RESET_CANCELLED))),
            "{ended:?}"
        );
        // What was in flight at the deadline: the opener's stream window
        // (pinc's 1.25 MiB) and its receive queue, never the rest.
        assert!(
            read <= IN_FLIGHT,
            "{read} bytes arrived after the deadline: the write went on"
        );
        opener.shutdown().await;
        server.shutdown().await;
    });
}

/// Takes its request and hangs without reading the rest of the stream, until
/// the node drops it at its deadline.
struct Stall {
    cut: mpsc::Sender<()>,
}

#[async_trait]
impl StreamHandler for Stall {
    async fn handle(&self, stream: &mut InboundStream) {
        let _guard = CutGuard(self.cut.clone());
        stream.request.release();
        std::future::pending::<()>().await;
    }
}

/// Writes `took`, finishes and returns without reading past its request. Its
/// guard tells `returned` once it has.
struct Take {
    returned: mpsc::Sender<()>,
}

#[async_trait]
impl StreamHandler for Take {
    async fn handle(&self, stream: &mut InboundStream) {
        let _guard = CutGuard(self.returned.clone());
        if stream.send.write_all(b"took").await.is_ok() {
            let _ = stream.send.finish().await;
        }
    }
}

/// Reads `recv` until it ends or fails.
async fn drain(recv: &mut crate::ProtocolRecvStream) -> Option<p2p::Error> {
    p2p::rt::time::timeout(IO_TIMEOUT, async {
        loop {
            match recv.read_chunk().await {
                Ok(Some(_)) => {}
                Ok(None) => return None,
                Err(error) => return Some(error),
            }
        }
    })
    .await
    .expect("the stream stayed open")
}

/// An upload-only opener finishes after the serving node cancelled the
/// stream at the handler's deadline: what it wrote was never taken, so its
/// finish fails rather than reading as delivered.
#[test]
fn a_finish_after_the_handler_was_cut_at_its_deadline_fails() {
    run(async {
        let (cut, mut was_cut) = mpsc::channel(4);
        let tag: &'static [u8] = b"/test/stall/1";
        let deadline = Duration::from_millis(200);
        let server = serving(protocol(tag, deadline), Arc::new(Stall { cut })).await;
        let opener = TestNode::spawn().await;
        let addrs = server.transport.listen_addresses().await.unwrap();
        let (mut send, mut recv) = opener
            .transport
            .open_stream(
                &server.id,
                addrs,
                &protocol(tag, deadline),
                b"upload".to_vec(),
            )
            .await
            .unwrap();
        p2p::rt::time::timeout(IO_TIMEOUT, was_cut.recv())
            .await
            .expect("the handler outlived its deadline");
        // The cut resets the reply half and stops the upload half together.
        let ended = drain(&mut recv).await;
        assert!(
            matches!(ended, Some(p2p::Error::StreamReset(RESET_CANCELLED))),
            "{ended:?}"
        );
        let finished = send.finish().await;
        assert!(
            matches!(finished, Err(p2p::Error::StreamReset(RESET_CANCELLED))),
            "a finish after the cancel read as {finished:?}"
        );
        opener.shutdown().await;
        server.shutdown().await;
    });
}

/// A handler that returns stops the stream with `STOP_HANDLED`, and the
/// opener's finish after that holds.
#[test]
fn a_finish_after_the_handler_returned_holds() {
    run(async {
        let (returned, mut has_returned) = mpsc::channel(4);
        let tag: &'static [u8] = b"/test/take/1";
        let deadline = Duration::from_secs(5);
        let server = serving(protocol(tag, deadline), Arc::new(Take { returned })).await;
        let opener = TestNode::spawn().await;
        let addrs = server.transport.listen_addresses().await.unwrap();
        let (mut send, mut recv) = opener
            .transport
            .open_stream(
                &server.id,
                addrs,
                &protocol(tag, deadline),
                b"upload".to_vec(),
            )
            .await
            .unwrap();
        p2p::rt::time::timeout(IO_TIMEOUT, has_returned.recv())
            .await
            .expect("the handler never returned");
        assert_eq!(recv.read_to_end(64).await.unwrap(), b"took");
        send.finish()
            .await
            .expect("a finish after the handler returned holds");
        opener.shutdown().await;
        server.shutdown().await;
    });
}
