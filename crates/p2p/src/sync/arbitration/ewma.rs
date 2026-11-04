//! The two fixed-point updates, the Q32 weight of a gap, and the packed word
//! that holds a time-decayed estimate (Lean: `roll`, `troll`, `weightQ32`).

use std::sync::atomic::{AtomicU64, Ordering};

use super::{TIME_BITS, VALUE_BITS, WEIGHT_ONE, WEIGHT_TABLE};

/// Largest value a packed estimate holds; larger samples saturate to it.
pub(crate) const VALUE_MAX: u64 = (1 << VALUE_BITS) - 1;

/// Largest stamp: the time of the last sample plus one.
const STAMP_MAX: u64 = (1 << TIME_BITS) - 1;

/// CAS attempts before a sample is dropped. A dropped sample only delays the
/// estimate; it never blocks the fetch that produced it.
const CAS_TRIES: usize = 8;

/// One more sample at weight `2^-shift`: `avg - avg / 2^s + sample / 2^s`.
/// The result lies between `avg` and `sample`, so it cannot overflow.
pub(crate) const fn roll(avg: u64, sample: u64, shift: u32) -> u64 {
    let keep = match avg.checked_shr(shift) {
        Some(part) => part,
        None => 0,
    };
    let add = match sample.checked_shr(shift) {
        Some(part) => part,
        None => 0,
    };
    avg - keep + add
}

/// Moves `v` a `w / WEIGHT_ONE` share of the way to `x`, the share's fraction
/// rounded by `dither`: down unless `dither` makes up the rest. Over a uniform
/// `dither` the move is the exact share (Lean `dither_mean`), so a share below
/// one unit still moves the estimate in proportion instead of freezing it. The
/// result lies between `v` and `x` for any weight and dither.
pub(crate) const fn troll(v: u64, x: u64, w: u64, dither: u64) -> u64 {
    let w = if w > WEIGHT_ONE { WEIGHT_ONE } else { w } as u128;
    let dither = (dither & (WEIGHT_ONE - 1)) as u128;
    let one = WEIGHT_ONE as u128;
    if v <= x {
        v + (((x - v) as u128 * w + dither) / one) as u64
    } else {
        v - (((v - x) as u128 * w + dither) / one) as u64
    }
}

/// `1 - 2^(-gap / half_life)` in Q32, never decreasing in `gap`.
///
/// Whole half-lives halve the kept share; within one, the share is
/// interpolated between two of the table's 64 steps, so even a 1 ms gap has a
/// weight and every gap is within about 1e-5 of exact.
pub(crate) const fn weight_q32(gap_ms: u64, half_life_ms: u64) -> u64 {
    if half_life_ms == 0 {
        return WEIGHT_ONE;
    }
    let halvings = gap_ms / half_life_ms;
    if halvings > 32 {
        return WEIGHT_ONE;
    }
    let position = (gap_ms % half_life_ms) as u128 * 64;
    let step = (position / half_life_ms as u128) as usize & 63;
    let fraction = position % half_life_ms as u128;
    let upper = WEIGHT_TABLE[step];
    let lower = if step < 63 {
        WEIGHT_TABLE[(step + 1) & 63]
    } else {
        WEIGHT_TABLE[0] / 2
    };
    let fall = ((upper - lower) as u128) << 16;
    let keep = ((upper as u128) << 16) - fall * fraction / half_life_ms as u128;
    WEIGHT_ONE - (keep >> halvings) as u64
}

/// A dither for the update of `word` at `now_ms`: deterministic, so a replay
/// reproduces it, and spread over `[0, WEIGHT_ONE)` (splitmix64).
const fn dither(word: u64, now_ms: u64) -> u64 {
    let mut z = (word ^ now_ms.rotate_left(32)).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) & (WEIGHT_ONE - 1)
}

/// An estimate's value and the time of its last sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Estimate {
    pub(crate) value: u64,
    pub(crate) at: u64,
}

/// A packed word: the time of the last sample plus one (zero means none yet)
/// above a 24-bit value.
pub(crate) const fn encode(at: u64, value: u64) -> u64 {
    let stamp = if at < STAMP_MAX { at + 1 } else { STAMP_MAX };
    let value = if value < VALUE_MAX { value } else { VALUE_MAX };
    stamp << VALUE_BITS | value
}

pub(crate) const fn decode(word: u64) -> Option<Estimate> {
    let stamp = word >> VALUE_BITS;
    if stamp == 0 {
        return None;
    }
    Some(Estimate {
        value: word & VALUE_MAX,
        at: stamp - 1,
    })
}

/// The word after a sample at `now_ms`: the first sample is taken as is,
/// later ones move the estimate by the weight of the gap since the last.
/// A clock that went backwards reads as no gap, so that sample weighs nothing
/// and the next one is weighed from `now_ms`.
pub(crate) const fn record_word(word: u64, now_ms: u64, sample: u64, half_life_ms: u64) -> u64 {
    let sample = if sample < VALUE_MAX {
        sample
    } else {
        VALUE_MAX
    };
    let value = match decode(word) {
        None => sample,
        Some(last) => troll(
            last.value,
            sample,
            weight_q32(now_ms.saturating_sub(last.at), half_life_ms),
            dither(word, now_ms),
        ),
    };
    encode(now_ms, value)
}

/// One time-decayed estimate, updated lock-free.
#[derive(Debug, Default)]
pub(crate) struct PackedEwma(AtomicU64);

impl PackedEwma {
    pub(crate) fn record(&self, now_ms: u64, sample: u64, half_life_ms: u64) {
        let mut word = self.0.load(Ordering::Relaxed);
        for _ in 0..CAS_TRIES {
            let next = record_word(word, now_ms, sample, half_life_ms);
            match self
                .0
                .compare_exchange_weak(word, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(current) => word = current,
            }
        }
    }

    pub(crate) fn read(&self) -> Option<Estimate> {
        decode(self.0.load(Ordering::Relaxed))
    }
}
