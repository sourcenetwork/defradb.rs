//! Loopback numbers, iroh against vinc in the same test binary: connect time,
//! and a replicator push's round trip at 1 and 16 concurrent streams.
//!
//! Ignored; run with
//! `cargo test -p p2p --features iroh-transport,vinc-transport --lib vinc::tests::bench -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use crate::message::PushLogReply;
use crate::signing::sign_with_transport;
use crate::sync::DispatchReceiver;
use crate::transport::{P2PTransport, TransportEvent};

const CONNECTS: usize = 40;
const REQUESTS_PER_STREAM: usize = 200;

fn percentile(samples: &mut [Duration], p: f64) -> Duration {
    samples.sort();
    let rank = ((samples.len() as f64 - 1.0) * p).round() as usize;
    samples[rank]
}

fn report(label: &str, samples: &mut [Duration]) {
    let p50 = percentile(samples, 0.50);
    let p99 = percentile(samples, 0.99);
    println!(
        "{label}: n={} p50={:.1}us p99={:.1}us",
        samples.len(),
        p50.as_secs_f64() * 1e6,
        p99.as_secs_f64() * 1e6
    );
}

/// Answer every push `events` carries until the channel closes.
async fn responder<T>(transport: T, mut events: DispatchReceiver<TransportEvent<T::ResponseToken>>)
where
    T: P2PTransport,
{
    while let Some(event) = events.recv().await {
        if let TransportEvent::TwoStreamRequest {
            request,
            token: Some(token),
            ..
        } = event
        {
            let mut reply = PushLogReply::success(&request.message_id);
            let _ = sign_with_transport(&transport, &mut reply);
            let _ = transport.send_pushlog_response(token, reply).await;
        }
    }
}

async fn timed_connect<T>(sender: &T, receiver: &T) -> Duration
where
    T: P2PTransport,
{
    let addrs = receiver.listen_addresses().await.unwrap();
    let started = Instant::now();
    sender.dial(receiver.local_peer_id(), addrs).await.unwrap();
    sender
        .poll_until_connected(receiver.local_peer_id(), Duration::from_secs(10))
        .await
        .unwrap();
    started.elapsed()
}

/// `streams` workers each push `REQUESTS_PER_STREAM` times; every round trip
/// is a sample.
async fn pushes<T>(sender: &T, receiver: &T, streams: usize) -> Vec<Duration>
where
    T: P2PTransport,
{
    let mut workers = Vec::new();
    for worker in 0..streams {
        let sender = sender.clone();
        let target = receiver.local_peer_id().clone();
        workers.push(crate::rt::spawn(async move {
            let mut samples = Vec::with_capacity(REQUESTS_PER_STREAM);
            for index in 0..REQUESTS_PER_STREAM {
                let mut request = crate::message::PushLogRequest::new(
                    format!("doc-{worker}-{index}"),
                    bytes::Bytes::from(vec![1, 2, 3]),
                    "collection1".to_string(),
                    "creator1".to_string(),
                    bytes::Bytes::from(vec![7u8; 512]),
                );
                sign_with_transport(&sender, &mut request).unwrap();
                let started = Instant::now();
                sender
                    .send_two_stream_request(&target, request)
                    .await
                    .unwrap();
                samples.push(started.elapsed());
            }
            samples
        }));
    }
    let mut all = Vec::new();
    for worker in workers {
        all.extend(worker.await);
    }
    all
}

async fn bench_vinc() {
    let mut connects = Vec::new();
    for _ in 0..CONNECTS {
        let a = super::TestNode::spawn().await;
        let b = super::TestNode::spawn().await;
        connects.push(timed_connect(&a.transport, &b.transport).await);
        a.shutdown().await;
        b.shutdown().await;
    }
    report("vinc connect", &mut connects);

    let a = super::TestNode::spawn().await;
    let b = super::TestNode::spawn().await;
    super::connect(&a.transport, &b.transport).await;
    let serve = crate::rt::spawn(responder(b.transport.clone(), b.events));
    // Warm the connection and the code paths before measuring.
    let _ = pushes(&a.transport, &b.transport, 1).await;
    for streams in [1, 16] {
        report(
            &format!("vinc pushlog streams={streams}"),
            &mut pushes(&a.transport, &b.transport, streams).await,
        );
    }
    serve.abort();
    a.transport.shutdown().await.unwrap();
    b.transport.shutdown().await.unwrap();
}

#[cfg(feature = "iroh-transport")]
mod iroh_side {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::iroh::{
        spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig, IrohRelayModeConfig, IrohTransport,
    };
    use crate::sync::DispatchReceiver;
    use crate::transport::TransportEvent;

    pub(super) struct IrohNode {
        pub(super) transport: IrohTransport,
        pub(super) events: DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>,
        task: edge::Task<()>,
    }

    impl IrohNode {
        pub(super) async fn spawn() -> Self {
            let secret_key = iroh::SecretKey::from_bytes(&super::super::seed());
            let config = IrohEndpointConfig {
                secret_key: secret_key.clone(),
                node_identity: None,
                relay_mode: IrohRelayModeConfig::Disabled,
                discovery: IrohDiscoveryConfig::Disabled,
                bind_port: None,
                bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                max_concurrent_multipath_paths: None,
                allowlist: Default::default(),
            };
            let (command_tx, events, _replicators, task) =
                spawn_endpoint(config, edge::testing::edge()).await.unwrap();
            Self {
                transport: IrohTransport::new(command_tx, secret_key),
                events,
                task,
            }
        }

        pub(super) async fn shutdown(self) {
            use crate::transport::P2PTransport as _;
            self.transport.shutdown().await.unwrap();
            let _ = self.task.await;
        }
    }
}

#[cfg(feature = "iroh-transport")]
async fn bench_iroh() {
    use iroh_side::IrohNode;

    let mut connects = Vec::new();
    for _ in 0..CONNECTS {
        let a = IrohNode::spawn().await;
        let b = IrohNode::spawn().await;
        connects.push(timed_connect(&a.transport, &b.transport).await);
        a.shutdown().await;
        b.shutdown().await;
    }
    report("iroh connect", &mut connects);

    let a = IrohNode::spawn().await;
    let b = IrohNode::spawn().await;
    timed_connect(&a.transport, &b.transport).await;
    b.transport
        .poll_until_connected(a.transport.local_peer_id(), Duration::from_secs(10))
        .await
        .unwrap();
    let serve = crate::rt::spawn(responder(b.transport.clone(), b.events));
    let _ = pushes(&a.transport, &b.transport, 1).await;
    for streams in [1, 16] {
        report(
            &format!("iroh pushlog streams={streams}"),
            &mut pushes(&a.transport, &b.transport, streams).await,
        );
    }
    serve.abort();
    a.transport.shutdown().await.unwrap();
    b.transport.shutdown().await.unwrap();
}

#[test]
#[ignore = "a benchmark: run it alone with --nocapture"]
fn loopback_connect_and_pushlog_iroh_against_vinc() {
    super::run(async {
        println!("loopback, one process, release-mode numbers only if built with --release");
        bench_vinc().await;
        #[cfg(feature = "iroh-transport")]
        bench_iroh().await;
    });
}
