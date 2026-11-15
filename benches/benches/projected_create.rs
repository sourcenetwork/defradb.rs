//! Bulk creates on an embedded node with node access control on and a
//! document projector installed that writes no NAC tuple.
//!
//! `cargo bench -p benches --bench projected_create`
//! creates 1, 100 and 1000 documents in one request (one transaction), as
//! the node owner, ten times each, on a fresh in-memory node per batch size.
//! The projector runs once per document; a transaction whose projector wrote
//! no NAC tuple should owe the edge snapshot nothing at commit. Override with
//! DEFRA_PROJECTED_BENCH_DOCS (comma-separated) and
//! DEFRA_PROJECTED_BENCH_REPETITIONS. One JSON line per sample on stdout.
//!
//! Allocations are counted across the whole process while the request runs,
//! divided by the documents.

use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_core::RequestContext;
use defra_node::{Node, QueryRequest};
use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{self, median, min_max, CountingAllocator};

#[global_allocator]
static ALLOC: CountingAllocator<mimalloc::MiMalloc> = CountingAllocator(mimalloc::MiMalloc);

const COLLECTION: &str = "Projected";

/// A projector that reads nothing and writes nothing.
struct Untouched;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for Untouched {
    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        _context: &db::DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }
}

fn counts(name: &str, default: &str) -> Vec<usize> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split(',')
        .map(|value| value.parse().expect(name))
        .collect()
}

/// One batch of `docs` creates in one request, timed, with its allocations.
async fn batch(node: &Node, owner: &RequestContext, docs: usize, round: usize) -> (Duration, u64) {
    let inputs: Vec<String> = (0..docs)
        .map(|doc| format!(r#"{{label: "round-{round}-doc-{doc}"}}"#))
        .collect();
    let mutation = format!(
        "mutation {{ create_{COLLECTION}(input: [{}]) {{ _docID }} }}",
        inputs.join(", ")
    );
    measure::reset();
    let start = Instant::now();
    let response = node
        .execute_request_as(owner, QueryRequest::new(mutation))
        .await
        .unwrap();
    let elapsed = start.elapsed();
    let allocations = measure::taken().count;
    assert!(
        !response.has_errors(),
        "create refused: {:?}",
        response.errors
    );
    (elapsed, allocations)
}

fn main() {
    let sizes = counts("DEFRA_PROJECTED_BENCH_DOCS", "1,100,1000");
    let repetitions = counts("DEFRA_PROJECTED_BENCH_REPETITIONS", "10")[0];
    let mut groups = [
        Group::lower_better("batch", "ms"),
        Group::higher_better("documents", "docs/s"),
        Group::lower_better("allocations", "allocations/doc"),
    ]
    .map(|group| group.over("documents per transaction"));
    workers::block_on(async {
        for &docs in &sizes {
            let node = Node::builder()
                .enable_signing()
                .with_node_acp_enabled()
                .with_document_projector(Arc::new(Untouched))
                .build()
                .await
                .unwrap();
            let owner = RequestContext::default()
                .with_identity(Some(node.node_identity_did().unwrap().to_owned()));
            node.add_collection_as(&owner, &format!("type {COLLECTION} {{ label: String }}"))
                .await
                .unwrap();
            let mut values: [Vec<f64>; 3] = Default::default();
            for repetition in 0..repetitions {
                let (elapsed, allocations) = batch(&node, &owner, docs, repetition).await;
                let metrics = [
                    elapsed.as_secs_f64() * 1000.0,
                    docs as f64 / elapsed.as_secs_f64(),
                    allocations as f64 / docs as f64,
                ];
                for (samples, value) in values.iter_mut().zip(metrics) {
                    samples.push(value);
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "projected_create_sample": {
                            "docs": docs,
                            "repetition": repetition,
                            "batch_ms": metrics[0],
                            "docs_per_s": metrics[1],
                            "allocations_per_doc": metrics[2],
                        }
                    })
                );
            }
            for (group, mut samples) in groups.iter_mut().zip(values) {
                let (lo, hi) = min_max(&samples);
                group.rows.push(
                    Row::new(docs.to_string(), median(&mut samples))
                        .range(lo, hi)
                        .at(docs as f64),
                );
            }
            node.shutdown().await.unwrap();
        }
    })
    .unwrap();
    let mut family = Family::new(
        "Projected bulk creates, node access control on",
        format!("An embedded in-memory node with node access control on and a document projector that writes no NAC tuple; one request creates the batch; {repetitions} batches per size. Allocations are the whole process's per document."),
    )
    .trust(Trust::Clean);
    for group in groups {
        family = family.group(group);
    }
    family.emit("projected_create");
}
