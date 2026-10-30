//! Concurrent writes through the HTTP execution path, on a fresh on-disk store
//! at Immediate durability for every sample. No network, signing or ACP.
//!
//! `cargo bench -p benches --bench write_baseline --profile release-fast`
//! runs 1, 4, 16 and 64 writers, 256 requests each, three times. Override with
//! DEFRA_BASELINE_WRITERS (comma-separated), DEFRA_BASELINE_OPERATIONS and
//! DEFRA_BASELINE_REPETITIONS. DEFRA_BASELINE_CHUNK_MAX_CHARS makes `left` a
//! `@chunk` field and emits the families with a `_chunked` suffix. Raw samples and provenance go to stdout as JSON
//! lines; dashboard families use DEFRA_BENCH_OUT through the existing emitter.

use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{median, min_max, percentile_ms};

mod common;
#[path = "write_baseline/workload.rs"]
mod workload;
use common::{counts, git, positive};
use workload::{Measurement, Scenario};

fn metrics(sample: &Measurement) -> Vec<(&'static str, &'static str, bool, f64)> {
    let before = sample.retries_before;
    let after = sample.retries_after;
    let commits = sample.commits as f64;
    assert!(commits > 0.0, "a sample must commit at least one request");
    vec![
        (
            "commits",
            "commits/s",
            false,
            commits / sample.elapsed.as_secs_f64(),
        ),
        (
            "storage conflicts",
            "conflicts/commit",
            true,
            sample.storage_conflicts as f64 / commits,
        ),
        (
            "client conflicts",
            "conflicts/commit",
            true,
            sample.client_conflicts as f64 / commits,
        ),
        ("HTTP retries (retired)", "retries/commit", true, 0.0),
        ("embedded retries (retired)", "retries/commit", true, 0.0),
        (
            "short ID reservation retries (retired)",
            "retries/commit",
            true,
            0.0,
        ),
        ("merge retries (retired)", "retries/commit", true, 0.0),
        (
            "engine retries",
            "retries/commit",
            true,
            sample.engine_retries as f64 / commits,
        ),
        (
            "push marker retries",
            "retries/commit",
            true,
            (after.push_marker.attempts - before.push_marker.attempts) as f64 / commits,
        ),
        (
            "client conflict requests",
            "requests",
            true,
            sample.client_conflicts as f64,
        ),
        (
            "engine exhausted requests",
            "requests",
            true,
            sample.engine_exhaustions as f64,
        ),
        (
            "latency p50",
            "ms",
            true,
            percentile_ms(&sample.latencies, 50),
        ),
        (
            "latency p99",
            "ms",
            true,
            percentile_ms(&sample.latencies, 99),
        ),
    ]
}

fn main() {
    let writers = counts("DEFRA_BASELINE_WRITERS", "1,4,16,64");
    let operations = positive("DEFRA_BASELINE_OPERATIONS", 256);
    let repetitions = positive("DEFRA_BASELINE_REPETITIONS", 3);
    let guard = common::load_guard();
    println!(
        "{}",
        serde_json::json!({
            "baseline_run": {
                "revision": git(&["rev-parse", "HEAD"]),
                "dirty": !git(&["status", "--porcelain"]).is_empty(),
                "host": defra_perf::run_meta::host(),
                "toolchain": defra_perf::run_meta::toolchain(),
                "profile": std::env::var("DEFRA_BASELINE_PROFILE").unwrap_or_else(|_| "unspecified".into()),
                "durability": "Immediate", "store": "disk", "writers": writers,
                "operations_per_writer": operations, "repetitions": repetitions,
            "shared_documents": workload::SHARED_DOCUMENTS,
                "chunk_max_chars": workload::chunk_max_chars(),
                "load_guard": guard,
            }
        })
    );
    let rt = common::owned_runtime();
    for scenario in Scenario::ALL {
        let mut groups: Vec<Group> = Vec::new();
        for &count in &writers {
            let mut values: Vec<Vec<f64>> = Vec::new();
            for repetition in 0..repetitions {
                let sample = rt.block_on(workload::run(scenario, count, operations));
                let metrics = metrics(&sample);
                if groups.is_empty() {
                    groups = metrics
                        .iter()
                        .map(|(name, unit, lower, _)| {
                            if *lower {
                                Group::lower_better(*name, *unit)
                            } else {
                                Group::higher_better(*name, *unit)
                            }
                            .over("writers")
                        })
                        .collect();
                }
                if values.is_empty() {
                    values.resize_with(metrics.len(), Vec::new);
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "baseline_sample": {
                            "workload": scenario.name(), "writers": count, "repetition": repetition,
                            "requests": sample.latencies.len(), "commits": sample.commits,
                            "client_conflicts": sample.client_conflicts,
                            "engine_exhaustions": sample.engine_exhaustions,
                            "elapsed_seconds": sample.elapsed.as_secs_f64(),
                            "metrics": metrics.iter().map(|(name, _, _, value)| (*name, *value)).collect::<std::collections::BTreeMap<_, _>>(),
                        }
                    })
                );
                for (samples, (_, _, _, value)) in values.iter_mut().zip(metrics) {
                    samples.push(value);
                }
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
        let mut family = Family::new(
            format!("Concurrent writes: {}", scenario.name()),
            format!("On-disk Immediate durability; {operations} requests per writer; {repetitions} fresh-store repetitions. HTTP execution without sockets, signing or ACP. Latencies include client-conflict and engine-exhausted requests. Storage conflicts exclude pre-commit query conflicts; client conflicts count requests returned with a transaction conflict. Engine retries count replay executions after a losing attempt; exhaustion is reported separately. Retired retry layers report zero by definition."),
        ).trust(if guard.passed { Trust::Clean } else { Trust::Contaminated });
        for group in groups {
            family = family.group(group);
        }
        let suffix = if workload::chunk_max_chars().is_some() {
            "_chunked"
        } else {
            ""
        };
        family.emit(&format!("write_baseline_{}{suffix}", scenario.name()));
    }
}
