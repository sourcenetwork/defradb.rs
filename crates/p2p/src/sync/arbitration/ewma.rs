//! The two fixed-point updates, the Q16 weight of a gap, and the packed word
//! that holds a time-decayed estimate (Lean: `roll`, `troll`, `weightQ16`).

use std::sync::atomic::{AtomicU64, Ordering};

use super::{LN2_Q16, TIME_BITS, VALUE_BITS, WEIGHT_ONE, WEIGHT_TABLE};

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

/// Moves `v` a `w / WEIGHT_ONE` share of the way to `x`, rounding toward `v`,
/// so the result lies between them for any weight.
pub(crate) const fn troll(v: u64, x: u64, w: u64) -> u64 {
    let w = if w > WEIGHT_ONE { WEIGHT_ONE } else { w } as u128;
    if v <= x {
        v + ((x - v) as u128 * w / WEIGHT_ONE as u128) as u64
    } else {
        v - ((v - x) as u128 * w / WEIGHT_ONE as u128) as u64
    }
}

/// `1 - 2^(-gap / half_life)` in Q16, never decreasing in `gap`.
///
/// Whole half-lives halve the kept share; the remainder picks one of 64 table
/// steps. A gap under one step is linear (`gap · ln 2 / half_life`), capped at
/// the first step so the weight never falls as the gap grows.
pub(crate) const fn weight_q16(gap_ms: u64, half_life_ms: u64) -> u64 {
    if half_life_ms == 0 {
        return WEIGHT_ONE;
    }
    let halvings = gap_ms / half_life_ms;
    if halvings > 16 {
        return WEIGHT_ONE;
    }
    let step = ((gap_ms % half_life_ms) as u128 * 64 / half_life_ms as u128) as usize & 63;
    if halvings == 0 && step == 0 {
        let linear = (gap_ms as u128 * LN2_Q16 as u128 / half_life_ms as u128) as u64;
        let first_step = WEIGHT_ONE - WEIGHT_TABLE[1];
        return if linear < first_step {
            linear
        } else {
            first_step
        };
    }
    WEIGHT_ONE - (WEIGHT_TABLE[step] >> halvings)
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
            weight_q16(now_ms.saturating_sub(last.at), half_life_ms),
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
