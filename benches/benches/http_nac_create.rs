//! Concurrent document creates over HTTP on an embedded node with node access
//! control on, through the real socket, auth middleware and NAC adapter.
//!
//! `cargo bench -p benches --bench http_nac_create`
//! runs 1, 4, 16 and 64 writers, 256 creates each (`POST
//! /api/v0/collections/{name}`), three times, each on a fresh on-disk node.
//! Every writer is the node owner, so every request passes the edge's node
//! checks and commits. Override with DEFRA_NAC_BENCH_WRITERS
//! (comma-separated), DEFRA_NAC_BENCH_OPERATIONS and
//! DEFRA_NAC_BENCH_REPETITIONS; DEFRA_NAC_BENCH_NAC=off runs the same
//! requests with node access control off. One JSON line per sample on stdout.
//!
//! Allocations are counted across the whole process (client, server and
//! database) while the writers run, divided by the requests: a ceiling to
//! compare between runs, not one request's own footprint.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_node::{HttpConfig, Node};
use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{self, median, min_max, percentile_ms, CountingAllocator};
use identity::{Identity as _, RawIdentity};

#[global_allocator]
static ALLOC: CountingAllocator<mimalloc::MiMalloc> = CountingAllocator(mimalloc::MiMalloc);

const COLLECTION: &str = "Sample";

struct Sample {
    elapsed: Duration,
    latencies: Vec<Duration>,
    commits: u64,
    allocations: u64,
}

fn positive(name: &str, default: usize) -> usize {
    let value = std::env::var(name).map_or(default, |s| s.parse().expect(name));
    assert!(value > 0, "{name} must be positive");
    value
}

fn counts(name: &str, default: &str) -> Vec<usize> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split(',')
        .map(|value| value.parse().expect(name))
        .collect()
}

fn bearer(identity: &RawIdentity, audience: &str) -> String {
    let token = identity::new_token(
        identity,
        Duration::from_secs(3600),
        Some(audience.into()),
        None,
    )
    .unwrap();
    format!("Bearer {}", String::from_utf8(token).unwrap())
}

/// The owner's DID, with its signing identity stored for the node to use.
fn owner_did(owner: &RawIdentity) -> String {
    let did = owner.did().unwrap();
    let public = owner.public_key_bytes();
    defra_core::signing::store_identity(
        did.as_str(),
        SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: owner.private_key_bytes(),
            public_key_bytes: public.clone(),
            public_key_hex: hex::encode(public),
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .expect("the store takes the identity");
    did.as_str().to_owned()
}

async fn run(client_edge: &edge::Edge, nac: bool, writers: usize, operations: usize) -> Sample {
    let owner = RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let did = owner_did(&owner);
    let directory = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut builder = Node::builder()
        .data_path(directory.path())
        .with_node_identity_did(&did)
        .with_http(HttpConfig::with_listener(listener).unwrap());
    if nac {
        builder = builder.with_node_acp_enabled();
    }
    let node = builder.build().await.unwrap();
    let context = defra_core::RequestContext::default().with_identity(Some(did));
    node.add_collection_as(&context, &format!("type {COLLECTION} {{ label: String }}"))
        .await
        .unwrap();

    let client = net::http::Client::builder(client_edge.clone())
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    let url = format!("http://{address}/api/v0/collections/{COLLECTION}");
    let auth = bearer(&owner, &address.to_string());

    measure::reset();
    let start = Instant::now();
    let tasks: Vec<_> = (0..writers)
        .map(|writer| {
            let client = client.clone();
            let url = url.clone();
            let auth = auth.clone();
            client_edge.spawn(async move {
                let mut latencies = Vec::with_capacity(operations);
                let mut commits = 0u64;
                for operation in 0..operations {
                    let body = format!(r#"{{"label":"writer-{writer}-operation-{operation}"}}"#);
                    let sent = Instant::now();
                    let response = client
                        .post(&url)
                        .header("Authorization", auth.as_str())
                        .header("Content-Type", "application/json")
                        .body(body)
                        .send()
                        .await
                        .unwrap();
                    let status = response.status().as_u16();
                    let text = response.text().await.unwrap();
                    latencies.push(sent.elapsed());
                    assert!(
                        (200..300).contains(&status),
                        "create refused: {status} {text}"
                    );
                    commits += 1;
                }
                (latencies, commits)
            })
        })
        .collect();
    let mut latencies = Vec::with_capacity(writers * operations);
    let mut commits = 0;
    for task in tasks {
        let (writer_latencies, writer_commits) = task.await.unwrap();
        latencies.extend(writer_latencies);
        commits += writer_commits;
    }
    let elapsed = start.elapsed();
    let allocations = measure::taken().count;
    node.shutdown().await.unwrap();
    Sample {
        elapsed,
        latencies,
        commits,
        allocations,
    }
}

fn main() {
    let writers = counts("DEFRA_NAC_BENCH_WRITERS", "1,4,16,64");
    let operations = positive("DEFRA_NAC_BENCH_OPERATIONS", 256);
    let repetitions = positive("DEFRA_NAC_BENCH_REPETITIONS", 3);
    let nac = std::env::var("DEFRA_NAC_BENCH_NAC").map_or(true, |value| value != "off");
    let client = edge::EdgeRuntime::start(edge::EdgeConfig {
        threads: NonZeroUsize::new(4),
        thread_name: "bench-client".to_string(),
        ..edge::EdgeConfig::default()
    })
    .expect("the client runtime starts");
    let client_edge = client.edge().clone();
    let guard = defra_perf::run_meta::load_guard(
        std::env::var_os("DEFRA_BASELINE_LOAD_GUARD")
            .as_deref()
            .map(std::path::Path::new),
    );
    let mut groups = [
        Group::higher_better("commits", "commits/s"),
        Group::lower_better("latency p50", "ms"),
        Group::lower_better("latency p99", "ms"),
        Group::lower_better("allocations", "allocations/op"),
    ]
    .map(|group| group.over("writers"));
    for &count in &writers {
        let mut values: [Vec<f64>; 4] = Default::default();
        for repetition in 0..repetitions {
            let sample = client_edge
                .block_on(run(&client_edge, nac, count, operations))
                .unwrap();
            let requests = sample.latencies.len() as f64;
            let metrics = [
                sample.commits as f64 / sample.elapsed.as_secs_f64(),
                percentile_ms(&sample.latencies, 50),
                percentile_ms(&sample.latencies, 99),
                sample.allocations as f64 / requests,
            ];
            for (samples, value) in values.iter_mut().zip(metrics) {
                samples.push(value);
            }
            println!(
                "{}",
                serde_json::json!({
                    "http_nac_create_sample": {
                        "nac": nac,
                        "writers": count,
                        "repetition": repetition,
                        "requests": sample.latencies.len(),
                        "commits_per_s": metrics[0],
                        "p50_ms": metrics[1],
                        "p99_ms": metrics[2],
                        "allocations_per_op": metrics[3],
                    }
                })
            );
        }
        for (group, mut samples) in groups.iter_mut().zip(values) {
            let (lo, hi) = min_max(&samples);
            group.rows.push(
                Row::new(count.to_string(), median(&mut samples))
                    .range(lo, hi)
                    .at(count as f64),
            );
        }
    }
    let state = if nac { "on" } else { "off" };
    let mut family = Family::new(
        format!("HTTP creates, node access control {state}"),
        format!("An embedded node on disk, its HTTP server over a local socket; {operations} creates per writer by the node owner; {repetitions} fresh-node repetitions. Allocations are the whole process's per request."),
    )
    .trust(if guard.passed { Trust::Clean } else { Trust::Contaminated });
    for group in groups {
        family = family.group(group);
    }
    family.emit(&format!("http_nac_create_{state}"));
    client.shutdown().unwrap();
}
