use std::time::Duration;

use defra_perf::measure::percentile_ms;

fn millis(range: std::ops::RangeInclusive<u64>) -> Vec<Duration> {
    range.rev().map(Duration::from_millis).collect()
}

#[test]
fn the_percentile_is_the_nearest_rank_of_the_sorted_samples() {
    let samples = millis(1..=100);
    assert_eq!(percentile_ms(&samples, 50), 50.0);
    assert_eq!(percentile_ms(&samples, 99), 99.0);
    assert_eq!(percentile_ms(&samples, 100), 100.0);
}

#[test]
fn a_rank_between_samples_rounds_up_to_the_next_one() {
    let samples = millis(1..=10);
    assert_eq!(percentile_ms(&samples, 50), 5.0);
    assert_eq!(percentile_ms(&samples, 99), 10.0);
    assert_eq!(percentile_ms(&samples, 1), 1.0);
}

#[test]
fn one_sample_is_every_percentile() {
    let samples = [Duration::from_micros(250)];
    assert_eq!(percentile_ms(&samples, 50), 0.25);
    assert_eq!(percentile_ms(&samples, 99), 0.25);
}

#[test]
#[should_panic(expected = "percentile of an empty sample")]
fn an_empty_sample_has_no_percentile() {
    percentile_ms(&[], 50);
}
