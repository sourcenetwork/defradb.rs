//! Property tests mirroring `proofs/lean/Arbitration`: each runs a seeded loop
//! over the lemma's quantifiers, and the fault cases drive the estimators past
//! their ranges to show they degrade instead of panicking.

use std::sync::Arc;
use std::time::Duration;

use rapidhash::{HashMapExt, RapidHashMap};

use super::due_order::tie_break_order;
use super::ewma::{decode, encode, record_word, troll, weight_q16};
use super::occupancy::advance;
use super::provider::expected_useful;
use super::*;

const CASES: usize = 20_000;
const H: u64 = 60_000;

/// splitmix64: deterministic, so a failure names its case.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

fn between(x: u64, a: u64, b: u64) -> bool {
    a.min(b) <= x && x <= a.max(b)
}

#[test]
fn roll_stays_between_its_inputs() {
    // roll_between_up, roll_between_down, roll_le_bound
    let mut rng = Rng(1);
    for case in 0..CASES {
        let (avg, sample) = if case % 2 == 0 {
            (rng.below(VALUE_MAX + 1), rng.below(VALUE_MAX + 1))
        } else {
            (rng.next(), rng.next())
        };
        for shift in [1, ROOT_SHIFT, OCCUPANCY_SHIFT, 63, 64, 200] {
            let rolled = roll(avg, sample, shift);
            assert!(between(rolled, avg, sample), "{avg} {sample} {shift}");
        }
    }
}

#[test]
fn troll_stays_between_its_inputs_for_any_weight() {
    // troll_between_up, troll_between_down, troll_le_bound
    let mut rng = Rng(2);
    for _ in 0..CASES {
        let (v, x) = (rng.below(VALUE_MAX + 1), rng.below(VALUE_MAX + 1));
        let w = rng.below(2 * WEIGHT_ONE);
        let moved = troll(v, x, w);
        assert!(between(moved, v, x), "{v} {x} {w}");
        assert!(moved <= VALUE_MAX);
        assert!(between(troll(rng.next(), rng.next(), w), 0, u64::MAX));
    }
}

#[test]
fn the_first_sample_is_taken_as_is() {
    // record_first, troll_full
    let mut rng = Rng(3);
    for _ in 0..CASES {
        let (sample, now) = (rng.below(VALUE_MAX + 1), rng.below(1 << 39));
        assert_eq!(
            decode(record_word(0, now, sample, H)).map(|e| e.value),
            Some(sample)
        );
        assert_eq!(troll(rng.below(VALUE_MAX), sample, WEIGHT_ONE), sample);
        assert_eq!(
            decode(advance(None, now, sample)).map(|e| e.value),
            Some(sample)
        );
    }
    let mut root = RootEstimates::default();
    root.observe(AttemptSample::Incomplete {
        duration: Duration::from_millis(7_000),
        fast: false,
        progressed: false,
    });
    assert_eq!(root.attempt_ms, Some(7_000));
    assert_eq!(root.no_progress_q16, Some(u16::MAX));
}

#[test]
fn a_steady_input_is_approached_monotonically_and_never_passed() {
    // iter_monotone_up, iter_stays_below; the falling case by symmetry
    let mut rng = Rng(4);
    for _ in 0..2_000 {
        let target = rng.below(VALUE_MAX + 1);
        let start = rng.below(target + 1);
        for shift in [ROOT_SHIFT, OCCUPANCY_SHIFT] {
            let mut avg = start;
            for _ in 0..300 {
                let next = roll(avg, target, shift);
                assert!(avg <= next && next <= target);
                avg = next;
            }
        }
        let mut word = encode(0, start);
        let mut now = 0;
        for _ in 0..300 {
            now += rng.below(3 * H);
            let next = record_word(word, now, target, H);
            let (before, after) = (decode(word).unwrap().value, decode(next).unwrap().value);
            assert!(before <= after && after <= target);
            word = next;
        }
    }
}

#[test]
fn a_step_decays_geometrically_to_the_dead_zone() {
    // decay_step, decay_geometric: w^k e_k <= (w-1)^k e_0 + (w-1) w^k
    let mut rng = Rng(5);
    for _ in 0..2_000 {
        for (shift, steps) in [(ROOT_SHIFT, 40u32), (OCCUPANCY_SHIFT, 8)] {
            let w = 1u128 << shift;
            let (target, start) = (rng.below(VALUE_MAX + 1), rng.below(VALUE_MAX + 1));
            let e0 = u128::from(target.abs_diff(start));
            let mut avg = start;
            for k in 1..=steps {
                avg = roll(avg, target, shift);
                let ek = u128::from(target.abs_diff(avg));
                assert!(
                    w.pow(k) * ek <= (w - 1).pow(k) * e0 + (w - 1) * w.pow(k),
                    "shift {shift} step {k}"
                );
            }
        }
    }
    // troll_decay_up, troll_decay_down: 65536 e' <= (65536 - w) e + 65536
    for _ in 0..CASES {
        let (v, x) = (rng.below(VALUE_MAX + 1), rng.below(VALUE_MAX + 1));
        let w = rng.below(WEIGHT_ONE + 1);
        let after = u128::from(x.abs_diff(troll(v, x, w)));
        let before = u128::from(x.abs_diff(v));
        let one = u128::from(WEIGHT_ONE);
        assert!(one * after <= (one - u128::from(w)) * before + one);
    }
}

#[test]
fn the_weight_tracks_its_half_life_and_never_falls() {
    // weight_le_one, weight_full_after_silence, weight_zero_gap; within 1/64
    let mut previous = 0;
    for gap in (0..20 * H).step_by(97) {
        let w = weight_q16(gap, H);
        assert!(w <= WEIGHT_ONE && w >= previous, "gap {gap}");
        previous = w;
        let exact = 1.0 - (-(gap as f64) / H as f64).exp2();
        assert!(
            (w as f64 / WEIGHT_ONE as f64 - exact).abs() < 1.0 / 64.0,
            "gap {gap}"
        );
    }
    assert_eq!(weight_q16(0, H), 0);
    assert_eq!(weight_q16(17 * H, H), WEIGHT_ONE);
    for (i, step) in WEIGHT_TABLE.iter().enumerate() {
        let exact = (WEIGHT_ONE as f64 * (-(i as f64) / 64.0).exp2()).floor() as u64;
        assert_eq!(*step, exact, "table step {i}");
    }
}

#[test]
fn derived_reads_stay_clamped() {
    // expected_useful_le, ppm_fits_value; samples saturate into the field
    let mut rng = Rng(6);
    for _ in 0..CASES {
        let cost = rng.below(VALUE_MAX * LATENCY_UNIT_MICROS + 1);
        let useful = rng.below(SCALE_PPM + 1);
        assert!(expected_useful(cost, useful) <= cost * 64);
        assert_eq!(expected_useful(cost, SCALE_PPM), cost);
        let sample = rng.next();
        assert_eq!(
            decode(encode(0, sample)).unwrap().value,
            sample.min(VALUE_MAX)
        );
    }
}

#[test]
fn the_tie_break_reorders_only_within_a_bucket() {
    // bucket_order_any_score, bucket_apart_keeps_order, inversion_within_bucket,
    // same_score_keeps_due_order
    let bucket = TIE_BUCKET.as_millis() as u64;
    let mut rng = Rng(7);
    for case in 0..2_000 {
        let len = 2 + rng.below(30) as usize;
        let mut offsets: Vec<u64> = (0..len).map(|_| rng.below(10 * bucket)).collect();
        offsets.sort_unstable();
        let same = case % 4 == 0;
        let due: Vec<(Duration, u16)> = offsets
            .iter()
            .map(|ms| {
                let score = if same { 7 } else { rng.below(1 << 16) as u16 };
                (Duration::from_millis(*ms), score)
            })
            .collect();
        let order = tie_break_order(&due);
        for (position, first) in order.iter().enumerate() {
            for later in &order[position + 1..] {
                if offsets[*first] > offsets[*later] {
                    assert!(offsets[*first] < offsets[*later] + bucket);
                    assert!(!same, "equal scores keep due order");
                }
            }
        }
    }
    assert!(!tie_break_reorders(&[(Duration::ZERO, 9), (TIE_BUCKET, 0)]));
    assert!(tie_break_reorders(&[
        (Duration::ZERO, 9),
        (TIE_BUCKET / 2, 0)
    ]));
}

#[test]
fn a_clock_that_goes_backwards_costs_one_sample() {
    let mut word = record_word(0, 10_000, 1_000, H);
    word = record_word(word, 4_000, 9_000, H);
    let skewed = decode(word).unwrap();
    assert_eq!(
        skewed.value, 1_000,
        "a zero gap weighs the sample at nothing"
    );
    assert_eq!(
        skewed.at, 4_000,
        "the next gap is measured on the new clock"
    );
    word = record_word(word, 4_000 + 5 * H, 9_000, H);
    assert!(decode(word).unwrap().value > 8_000);

    let tick = advance(None, 50, 256);
    let back = decode(advance(Some(tick), 3, 0)).unwrap();
    assert_eq!((back.value, back.at), (256, 3));

    let estimates = ProviderEstimates::default();
    estimates.record(10_000, Duration::from_millis(40), true);
    estimates.record(1, Duration::from_millis(400), false);
    let export = estimates.export(0);
    assert_eq!(export.useful_ewma_ppm, Some(SCALE_PPM));
    assert_eq!(export.ewma_age_millis, Some(0));
}

#[test]
fn saturated_inputs_degrade_instead_of_panicking() {
    for half_life in [0, 1, H, u64::MAX] {
        for gap in [0, 1, H, u64::MAX - 1, u64::MAX] {
            assert!(weight_q16(gap, half_life) <= WEIGHT_ONE);
            let word = record_word(encode(u64::MAX, VALUE_MAX), gap, u64::MAX, half_life);
            assert!(decode(word).unwrap().value <= VALUE_MAX);
        }
    }
    let at_end = decode(encode(u64::MAX, u64::MAX)).unwrap();
    assert_eq!(at_end.value, VALUE_MAX);
    assert_eq!(at_end.at, (1 << TIME_BITS) - 2);

    let estimates = ProviderEstimates::default();
    estimates.record(u64::MAX, Duration::MAX, true);
    estimates.record(u64::MAX, Duration::MAX, false);
    let export = estimates.export(u64::MAX);
    assert_eq!(
        export.success_latency_micros,
        Some(VALUE_MAX * LATENCY_UNIT_MICROS)
    );
    assert!(export.expected_useful_micros.unwrap() <= VALUE_MAX * LATENCY_UNIT_MICROS * 64);

    let mut root = RootEstimates::default();
    for _ in 0..3 {
        root.observe(AttemptSample::Incomplete {
            duration: Duration::MAX,
            fast: false,
            progressed: false,
        });
    }
    assert_eq!(root.attempt_ms, Some(u32::MAX));
    assert_eq!(root.no_progress_ppm(), Some(SCALE_PPM));

    let occupancy = PendingOccupancy::default();
    let mut held = RapidHashMap::new();
    held.insert(Arc::<str>::from("peer"), usize::MAX);
    occupancy.sample(u64::MAX, &held);
    occupancy.sample(0, &held);
    assert_eq!(occupancy.snapshot(&held)[0].occupancy_q8, VALUE_MAX);
}

#[test]
fn a_contended_word_never_tears() {
    let word = Arc::new(PackedEwma::default());
    let threads: Vec<_> = (0..8u64)
        .map(|thread| {
            let word = Arc::clone(&word);
            std::thread::spawn(move || {
                for now in 0..5_000u64 {
                    word.record(now, 1_000 + thread * 100, H);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let estimate = word.read().unwrap();
    assert!(between(estimate.value, 1_000, 1_700));
    assert!(estimate.at < 5_000);
}

#[test]
fn shed_and_fast_refused_attempts_never_feed_the_backoff_estimate() {
    let mut root = RootEstimates::default();
    root.observe(AttemptSample::Shed);
    root.observe(AttemptSample::Incomplete {
        duration: Duration::from_millis(400),
        fast: true,
        progressed: false,
    });
    root.observe(AttemptSample::Skipped);
    root.observe(AttemptSample::Preempted);
    assert_eq!(root.attempt_ms, None);
    assert_eq!(root.no_progress_q16, Some(32_768));
    root.observe(AttemptSample::Shed);
    assert_eq!(root.no_progress_q16, Some(49_151));
    root.observe(AttemptSample::Incomplete {
        duration: Duration::from_secs(8),
        fast: false,
        progressed: true,
    });
    assert_eq!(root.attempt_ms, Some(8_000));
    assert_eq!(root.no_progress_q16, Some(24_576));
}

#[test]
fn occupancy_halves_in_about_177_seconds() {
    // occupancy_half_life_ticks
    let full = 100 * 256;
    let mut word = advance(None, 0, full);
    for tick in 1..=88 {
        word = advance(Some(word), tick, 0);
    }
    assert!(decode(word).unwrap().value > full / 2);
    word = advance(Some(word), 89, 0);
    assert!(decode(word).unwrap().value <= full / 2);
    assert_eq!(
        decode(advance(Some(encode(0, full)), 89, 0)).unwrap().value,
        decode(word).unwrap().value,
        "missed ticks roll in as if sampled"
    );
}

#[path = "../../../tests/unit/estimator_trace_replay.rs"]
mod trace_replay;
