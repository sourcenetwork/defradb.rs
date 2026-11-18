//! The fleet's recorded fetches, driven through the real `ProviderEstimates`
//! on the trace's own clock.
//!
//! `testdata/fetch_trace.csv` holds de-smoothed 30 s scrape intervals from the
//! fleet's VictoriaMetrics series (2026-11-03): two spoke→hub links across
//! studio-1's 14:53 restart and the rolling rc1 upgrade, and one hub→spoke
//! link over 8 h. Each row is expanded into `n` fetches spread over its
//! interval with the row's outcome mix. Latency is the lower-bound proxy
//! `min_rtt + bytes / delivery_rate`; refusals cost the RTT, timeouts 30 s.

use std::time::Duration;

use crate::sync::arbitration::{roll, ProviderEstimates, PROVIDER_HALF_LIFE, SCALE_PPM};

const TRACE: &str = include_str!("../../testdata/fetch_trace.csv");
const H_MS: u64 = PROVIDER_HALF_LIFE.as_millis() as u64;
const OUTAGE_MS: u64 = 15 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Useful,
    Busy,
    TimedOut,
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct Fetch {
    at_ms: u64,
    outcome: Outcome,
    cost: Duration,
}

fn field(row: &[&str], index: usize) -> Option<u64> {
    row.get(index).and_then(|value| value.parse().ok())
}

/// One row's fetches: counts by largest remainder, interleaved so each outcome
/// is spread over the interval.
fn expand(row: &[&str]) -> Vec<Fetch> {
    let n = field(row, 3).unwrap_or(0);
    let (end, length) = (field(row, 1).unwrap_or(0), field(row, 2).unwrap_or(0));
    let (useful, busy, timeout) = (
        field(row, 4).unwrap_or(0),
        field(row, 5).unwrap_or(0),
        field(row, 6).unwrap_or(0),
    );
    let failed = SCALE_PPM.saturating_sub(useful + busy + timeout);
    let shares = [
        (Outcome::Useful, useful),
        (Outcome::Busy, busy),
        (Outcome::TimedOut, timeout),
        (Outcome::Failed, failed),
    ];
    let mut counts: Vec<u64> = shares.iter().map(|(_, s)| s * n / SCALE_PPM).collect();
    let mut by_remainder: Vec<usize> = (0..4).collect();
    by_remainder.sort_by_key(|&i| (std::cmp::Reverse(shares[i].1 * n % SCALE_PPM), i));
    let short = n - counts.iter().sum::<u64>();
    for &i in by_remainder.iter().take(short as usize) {
        counts[i] += 1;
    }
    let (latency, rtt) = (field(row, 7), field(row, 8));
    let mut owed = [0i64; 4];
    (0..n)
        .map(|i| {
            for (owe, count) in owed.iter_mut().zip(&counts) {
                *owe += *count as i64;
            }
            let mut pick = 0;
            for k in 1..4 {
                if owed[k] > owed[pick] {
                    pick = k;
                }
            }
            owed[pick] -= n as i64;
            let outcome = shares[pick].0;
            let micros = match outcome {
                Outcome::Useful => latency.or(rtt).unwrap_or(50_000),
                Outcome::TimedOut => 30_000_000,
                Outcome::Busy | Outcome::Failed => rtt.unwrap_or(1_000),
            };
            Fetch {
                at_ms: end - length + (2 * i + 1) * length / (2 * n),
                outcome,
                cost: Duration::from_micros(micros),
            }
        })
        .collect()
}

fn links() -> Vec<(String, Vec<Fetch>)> {
    let mut links: Vec<(String, Vec<Fetch>)> = Vec::new();
    for line in TRACE.lines().skip(1) {
        let row: Vec<&str> = line.split(',').collect();
        let name = row[0].to_string();
        if links.last().is_none_or(|(last, _)| *last != name) {
            links.push((name, Vec::new()));
        }
        if let Some((_, fetches)) = links.last_mut() {
            fetches.extend(expand(&row));
        }
    }
    links
}

fn useful_ppm(estimates: &ProviderEstimates, at_ms: u64) -> u64 {
    estimates.export(at_ms).useful_ewma_ppm.unwrap_or(SCALE_PPM)
}

/// Crossings of one half, and flaps of the 0.4 / 0.6 band.
fn flaps(shares: &[u64]) -> (usize, usize) {
    let (mut crossings, mut band_flaps) = (0, 0);
    let (mut below, mut demoted) = (None, None);
    for &share in shares {
        let now_below = share < SCALE_PPM / 2;
        crossings += usize::from(below.is_some_and(|was| was != now_below));
        below = Some(now_below);
        let was = demoted.unwrap_or(now_below);
        let now = if !was && share < SCALE_PPM * 4 / 10 {
            true
        } else if was && share > SCALE_PPM * 6 / 10 {
            false
        } else {
            was
        };
        band_flaps += usize::from(demoted.is_some_and(|was| was != now));
        demoted = Some(now);
    }
    (crossings, band_flaps)
}

#[test]
fn every_estimate_stays_within_the_samples_it_was_fed() {
    for (link, fetches) in links() {
        let estimates = ProviderEstimates::default();
        let units = |cost: Duration| cost.as_micros() as u64 / 4 * 4;
        let (mut latency, mut cost) = ((u64::MAX, 0), (u64::MAX, 0));
        for fetch in &fetches {
            let useful = fetch.outcome == Outcome::Useful;
            estimates.record(fetch.at_ms, fetch.cost, useful);
            let sample = units(fetch.cost);
            if useful {
                latency = (latency.0.min(sample), latency.1.max(sample));
            }
            cost = (cost.0.min(sample), cost.1.max(sample));
            let export = estimates.export(fetch.at_ms);
            if let Some(estimate) = export.success_latency_micros {
                assert!((latency.0..=latency.1).contains(&estimate), "{link}");
            }
            let estimate = export.attempt_cost_micros.unwrap_or_default();
            assert!((cost.0..=cost.1).contains(&estimate), "{link}");
            assert!(export.useful_ewma_ppm.unwrap_or_default() <= SCALE_PPM);
        }
    }
}

#[test]
fn the_useful_share_flaps_less_than_todays_per_sample_average() {
    for (link, fetches) in links().into_iter().filter(|(l, _)| l.starts_with("spoke")) {
        let estimates = ProviderEstimates::default();
        let mut today = None;
        let (mut decayed, mut per_sample) = (Vec::new(), Vec::new());
        for fetch in &fetches {
            let useful = fetch.outcome == Outcome::Useful;
            estimates.record(fetch.at_ms, fetch.cost, useful);
            decayed.push(useful_ppm(&estimates, fetch.at_ms));
            let sample = if useful { SCALE_PPM } else { 0 };
            today = Some(today.map_or(sample, |avg| roll(avg, sample, 3)));
            per_sample.push(today.unwrap_or_default());
        }
        let span = fetches.last().map_or(0, |f| f.at_ms) - fetches.first().map_or(0, |f| f.at_ms);
        let hours = span as f64 / 3_600_000.0;
        let (crossings, band) = flaps(&decayed);
        let (today_crossings, today_band) = flaps(&per_sample);
        assert!(
            band as f64 / hours <= 8.0,
            "{link}: {band} band flaps in {hours:.1} h"
        );
        assert!(band <= today_band, "{link}: {band} vs {today_band}");
        assert!(
            2 * crossings <= today_crossings,
            "{link}: {crossings} vs {today_crossings}"
        );
    }
}

#[test]
fn an_outage_is_demoted_within_a_half_life_and_forgiven_within_two() {
    let mut checked = 0;
    for (link, fetches) in links() {
        let (first, last) = match (fetches.first(), fetches.last()) {
            (Some(first), Some(last)) => (first.at_ms, last.at_ms),
            _ => continue,
        };
        let mut start = first + 10 * 60_000;
        while start + 2 * OUTAGE_MS < last {
            let before = fetches.partition_point(|f| f.at_ms < start);
            let after = fetches.partition_point(|f| f.at_ms < start + OUTAGE_MS);
            let recovered = fetches.partition_point(|f| f.at_ms < start + 2 * OUTAGE_MS);
            let estimates = ProviderEstimates::default();
            for fetch in &fetches[..before] {
                estimates.record(fetch.at_ms, fetch.cost, fetch.outcome == Outcome::Useful);
            }
            let healthy = before > 0 && useful_ppm(&estimates, start) > SCALE_PPM * 6 / 10;
            if healthy && after > before {
                let gap = |range: &[Fetch], from: u64| {
                    range
                        .iter()
                        .scan(from, |last, f| {
                            Some(f.at_ms - std::mem::replace(last, f.at_ms))
                        })
                        .max()
                        .unwrap_or(0)
                };
                let mut demoted = None;
                for fetch in &fetches[before..after] {
                    estimates.record(fetch.at_ms, fetch.cost, false);
                    if demoted.is_none() && useful_ppm(&estimates, fetch.at_ms) < SCALE_PPM / 2 {
                        demoted = Some(fetch.at_ms - start);
                    }
                }
                let bound = H_MS + gap(&fetches[before..after], start);
                assert!(
                    demoted.is_some_and(|d| d <= bound),
                    "{link} at {start}: {demoted:?}"
                );
                let natural = &fetches[after..recovered];
                let useful = natural
                    .iter()
                    .filter(|f| f.outcome == Outcome::Useful)
                    .count();
                if !natural.is_empty() && useful * 100 >= natural.len() * 65 {
                    let restored = natural.iter().find_map(|fetch| {
                        estimates.record(fetch.at_ms, fetch.cost, fetch.outcome == Outcome::Useful);
                        (useful_ppm(&estimates, fetch.at_ms) >= SCALE_PPM / 2)
                            .then_some(fetch.at_ms - start - OUTAGE_MS)
                    });
                    let bound = 2 * H_MS + gap(natural, start + OUTAGE_MS);
                    assert!(restored.is_some_and(|r| r <= bound), "{link} at {start}");
                }
                checked += 1;
            }
            start += 30 * 60_000;
        }
    }
    assert!(
        checked >= 10,
        "only {checked} outages had a healthy provider to demote"
    );
}
