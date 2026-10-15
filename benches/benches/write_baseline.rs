//! Concurrent writes through the HTTP execution path, on a fresh on-disk store
//! at Immediate durability for every sample. No network, signing or ACP.
//!
//! `cargo bench -p benches --bench write_baseline --profile release-fast`
//! runs 1, 4, 16 and 64 writers, 256 requests each, three times. Override with
//! DEFRA_BASELINE_WRITERS (comma-separated), DEFRA_BASELINE_OPERATIONS and
//! DEFRA_BASELINE_REPETITIONS. Raw samples and provenance go to stdout as JSON
//! lines; dashboard families use DEFRA_BENCH_OUT through the existing emitter.

use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{median, min_max};
use std::time::Duration;

mod common;
#[path = "write_baseline/workload.rs"]
mod workload;
use workload::{Measurement, Scenario};

fn positive(name: &str, default: usize) -> usize {
    let value = std::env::var(name).map_or(default, |s| s.parse().expect(name));
    assert!(value > 0, "{name} must be positive");
    value
}

fn percentile(samples: &[Duration], percent: usize) -> f64 {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * percent).div_ceil(100) - 1].as_secs_f64() * 1000.0
}

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
            "HTTP conflicts",
            "conflicts/commit",
            true,
            (after.http_auto_commit.attempts - before.http_auto_commit.attempts
                + after.http_auto_commit.exhaustions
                - before.http_auto_commit.exhaustions) as f64
                / commits,
        ),
        (
            "HTTP retries",
            "retries/commit",
            true,
            (after.http_auto_commit.attempts - before.http_auto_commit.attempts) as f64 / commits,
        ),
        (
            "embedded retries",
            "retries/commit",
            true,
            (after.embedded_execute.attempts - before.embedded_execute.attempts) as f64 / commits,
        ),
        (
            "merge retries",
            "retries/commit",
            true,
            (after.merge.attempts - before.merge.attempts) as f64 / commits,
        ),
        (
            "push marker retries",
            "retries/commit",
            true,
            (after.push_marker.attempts - before.push_marker.attempts) as f64 / commits,
        ),
        (
            "exhausted requests",
            "requests",
            true,
            sample.exhaustions as f64,
        ),
        ("latency p50", "ms", true, percentile(&sample.latencies, 50)),
        ("latency p99", "ms", true, percentile(&sample.latencies, 99)),
    ]
}

fn main() {
    let writers: Vec<usize> = std::env::var("DEFRA_BASELINE_WRITERS")
        .unwrap_or_else(|_| "1,4,16,64".into())
        .split(',')
        .map(|value| {
            value
                .parse()
                .expect("positive comma-separated writer counts")
        })
        .collect();
    assert!(writers.iter().all(|count| *count > 0));
    let operations = positive("DEFRA_BASELINE_OPERATIONS", 256);
    let repetitions = positive("DEFRA_BASELINE_REPETITIONS", 3);
    let guard = defra_perf::run_meta::load_guard(
        std::env::var_os("DEFRA_BASELINE_LOAD_GUARD")
            .as_deref()
            .map(std::path::Path::new),
    );
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
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
            "max_http_retries": workload::MAX_HTTP_RETRIES,
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
            format!("On-disk Immediate durability; {operations} requests per writer; {repetitions} fresh-store repetitions. HTTP execution including retry backoff, without sockets, signing or ACP. Latencies include exhausted requests. Storage conflicts exclude pre-commit query conflicts; HTTP conflicts count both retries and exhausted requests. Zero retries at unused layers means those layers were not exercised."),
        ).trust(if guard.passed { Trust::Clean } else { Trust::Contaminated });
        for group in groups {
            family = family.group(group);
        }
        family.emit(&format!("write_baseline_{}", scenario.name()));
    }
}
