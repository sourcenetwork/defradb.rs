//! The manual probe for choosing the epoch tick period.

use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use futures::{StreamExt, TryStreamExt};
use serde_json::json;

use super::{one_doc, pipeline, IDENTITY_PASSTHROUGH_WASM, INFINITE_TRANSFORM_WASM};
use crate::engine::{LensEngine, ThreadTicks, WasmSandboxConfig};
use crate::Error;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * quantile).round() as usize;
    sorted[index]
}

/// Manual performance probe for choosing the epoch ticker period. Run with:
/// `cargo test -p lens measure_epoch_ticker_periods -- --ignored --nocapture`.
/// `DEFRA_LENS_TICKER_SAMPLES` and `DEFRA_LENS_TICKER_LIGHT_MS` shorten or
/// lengthen the run. It reports wall latency only; portable Rust does not
/// expose reliable thread CPU-time or context-switch counters here.
#[test]
#[ignore = "manual epoch ticker timing probe; run on an otherwise idle host"]
fn measure_epoch_ticker_periods() {
    let samples = env_usize("DEFRA_LENS_TICKER_SAMPLES", 20);
    let light_sample_ms = env_usize("DEFRA_LENS_TICKER_LIGHT_MS", 2_000);
    let light_fuel_budget = env_u64("DEFRA_LENS_TICKER_FUEL", u64::MAX);
    let docs_per_batch = 16;
    let periods_ms = [10, 25, 50, 100, 250];
    let input_docs = vec![
        json!({"id": "doc-1", "name": "Ada", "count": 7}),
        json!({"id": "doc-2", "name": "Grace", "count": 11}),
    ];

    println!("epoch ticker probe: profile={}, samples={samples}, light_ms/sample={light_sample_ms}, docs/batch={docs_per_batch}, light_fuel={light_fuel_budget}, trap_fuel=u64::MAX, epoch_deadline_ticks=2", if cfg!(debug_assertions) { "debug" } else { "release" });
    println!("period_ms,light_median_ns_per_doc,light_p95_ns_per_doc,light_epoch_traps,trap_median_ms,trap_p95_ms,trap_min_ms,trap_max_ms");
    println!("CPU time/context switches: not measured by this portable harness; optionally wrap the test process in platform-specific perf tooling.");

    for period_ms in periods_ms {
        let period = Duration::from_millis(period_ms);
        let mut sandbox = WasmSandboxConfig::restrictive();
        // Use the actual two-tick deadline for ordinary work too. The
        // light probe runs long enough to span many timer periods and
        // reports any accidental interruption. Fuel is only a distant
        // safety bound so it does not mask epoch timing.
        sandbox.fuel_budget = Some(light_fuel_budget);
        sandbox.epoch_deadline_ticks = Some(2);
        sandbox.epoch_tick_period = Some(period);
        let engine = LensEngine::new(Some(sandbox), &ThreadTicks).unwrap();
        let identity = pipeline(&engine, IDENTITY_PASSTHROUGH_WASM);
        let run = |docs: Vec<serde_json::Value>| -> crate::Result<Vec<serde_json::Value>> {
            block_on(
                engine
                    .call(
                        Arc::clone(&identity),
                        futures::stream::iter(docs),
                        false,
                        None,
                    )
                    .try_collect(),
            )
        };

        // Warm module instantiation, host marshalling, and the timer before sampling.
        for _ in 0..5 {
            assert_eq!(run(input_docs.clone()).unwrap(), input_docs);
        }

        let mut light_ns_per_doc = Vec::with_capacity(samples);
        let mut light_epoch_traps = 0usize;
        let mut light_traps_per_sample = Vec::with_capacity(samples);
        for _ in 0..samples {
            let started = std::time::Instant::now();
            let mut doc_count = 0usize;
            let mut sample_traps = 0usize;
            while started.elapsed() < Duration::from_millis(light_sample_ms as u64) {
                let docs = (0..docs_per_batch)
                    .map(|index| input_docs[index % input_docs.len()].clone())
                    .collect();
                match run(docs) {
                    Ok(result) => doc_count += result.len(),
                    Err(Error::TimeLimit) => {
                        light_epoch_traps += 1;
                        sample_traps += 1;
                    }
                    Err(error) => panic!("light transform failed unexpectedly: {error}"),
                }
            }
            light_ns_per_doc.push(started.elapsed().as_nanos() as f64 / doc_count.max(1) as f64);
            light_traps_per_sample.push(sample_traps);
        }
        let raw_light_ns_per_doc = light_ns_per_doc.clone();
        light_ns_per_doc.sort_by(f64::total_cmp);

        let endless = pipeline(&engine, INFINITE_TRANSFORM_WASM);
        let mut trap_ms = Vec::with_capacity(samples);
        for _ in 0..samples {
            let engine = engine.clone();
            let endless = Arc::clone(&endless);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let outcome = block_on(engine.call(endless, one_doc(), false, None).next());
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                let _ = sender.send((elapsed_ms, matches!(outcome, Some(Err(Error::TimeLimit)))));
            });
            let completed = receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("infinite transform must trap within the safety timeout");
            assert!(
                completed.1,
                "epoch deadline should interrupt before fuel fallback"
            );
            trap_ms.push(completed.0);
        }
        let raw_trap_ms = trap_ms.clone();
        trap_ms.sort_by(f64::total_cmp);

        println!(
            "{period_ms},{:.1},{:.1},{light_epoch_traps},{:.3},{:.3},{:.3},{:.3}",
            percentile(&light_ns_per_doc, 0.50),
            percentile(&light_ns_per_doc, 0.95),
            percentile(&trap_ms, 0.50),
            percentile(&trap_ms, 0.95),
            trap_ms[0],
            trap_ms[trap_ms.len() - 1],
        );
        for sample_index in 0..samples {
            println!(
                "raw_sample,{period_ms},{sample_index},{:.1},{},{:.3}",
                raw_light_ns_per_doc[sample_index],
                light_traps_per_sample[sample_index],
                raw_trap_ms[sample_index],
            );
        }
    }
}
