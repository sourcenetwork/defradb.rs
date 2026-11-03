//! Estimators for DAG-fetch arbitration: per provider, how long a useful fetch
//! takes and how often one is useful; per pending root, how long its attempts
//! run and whether they advance it; per source peer, how much of the pending
//! map it holds.
//!
//! They are measured and exported through the sync status only: nothing reads
//! them to rank, pace, order or admit. Every fault (a clock going backwards, a
//! saturated value, a CAS that keeps losing) costs at most a sample, never a
//! panic. The updates are proved in `proofs/lean/Arbitration.lean`, whose
//! constants `proofs/tests/lean_conformance.rs` binds to the ones below.

use std::time::Duration;

use n0_future::time::Instant;

mod due_order;
mod ewma;
mod occupancy;
mod provider;
mod root;

pub(crate) use due_order::tie_break_reorders;
pub use due_order::TIE_BUCKET;
pub(crate) use ewma::{roll, PackedEwma, VALUE_MAX};
pub(crate) use occupancy::PendingOccupancy;
pub use occupancy::PendingPeerOccupancy;
pub use provider::{ProviderEstimateExport, ProviderEstimates};
pub(crate) use root::AttemptSample;
pub use root::RootEstimates;

/// Bits of a packed estimate that hold its value.
pub const VALUE_BITS: u32 = 24;

/// Bits of a packed estimate that hold the time of its last sample, in
/// milliseconds (or ticks) since its owner's epoch: 34.8 years of ms.
pub const TIME_BITS: u32 = 40;

/// A weight of one, in Q32.
pub const WEIGHT_ONE: u64 = 1 << 32;

/// `⌊65536 · 2^(-i/64)⌋`: the share of an estimate, in Q16, that a gap of
/// `i/64` half-lives keeps; `weight_q32` interpolates between the steps.
pub const WEIGHT_TABLE: [u64; 64] = [
    65536, 64830, 64131, 63440, 62757, 62081, 61412, 60751, 60096, 59449, 58809, 58175, 57548,
    56928, 56315, 55709, 55108, 54515, 53928, 53347, 52772, 52204, 51641, 51085, 50535, 49990,
    49452, 48919, 48392, 47871, 47355, 46845, 46340, 45841, 45347, 44859, 44376, 43898, 43425,
    42957, 42494, 42037, 41584, 41136, 40693, 40254, 39821, 39392, 38967, 38548, 38132, 37722,
    37315, 36913, 36516, 36122, 35733, 35348, 34968, 34591, 34218, 33850, 33485, 33124,
];

/// Half-life of the per-provider estimators, in time rather than samples:
/// sample rates differ about 5000× between the fleet's links.
pub const PROVIDER_HALF_LIFE: Duration = Duration::from_secs(60);

/// Per-root estimators weigh each attempt one part in `2^ROOT_SHIFT` (α = ½):
/// a root sees a handful of attempts, so the latest dominates.
pub const ROOT_SHIFT: u32 = 1;

/// Occupancy is sampled once per `OCCUPANCY_TICK`, weighted one part in
/// `2^OCCUPANCY_SHIFT`: a half-life of 88.4 ticks, 177 s.
pub const OCCUPANCY_SHIFT: u32 = 7;
pub const OCCUPANCY_TICK: Duration = Duration::from_secs(2);

/// Unit of the latency and cost estimates: 24 bits of it cover 67 s.
pub const LATENCY_UNIT_MICROS: u64 = 4;

/// A share of one, in parts per million.
pub const SCALE_PPM: u64 = 1_000_000;

/// Floor of the useful share when reading the expected time to a useful
/// fetch, so a provider that is never useful costs 64 attempts, not infinity.
pub const USEFUL_FLOOR_PPM: u64 = SCALE_PPM / 64;

/// The backoff floor the tie-break bucket must not exceed, so a root re-armed
/// at the end of its attempt still goes behind every root that became due
/// during it.
pub const BACKOFF_FLOOR: Duration = crate::sync::manager::pending::PENDING_RETRY_BASE;

const _: () = assert!(VALUE_BITS + TIME_BITS == 64);
const _: () = assert!(SCALE_PPM <= VALUE_MAX);
const _: () = assert!(USEFUL_FLOOR_PPM * 64 == SCALE_PPM);
const _: () = assert!(TIE_BUCKET.as_millis() >= 1);
const _: () = assert!(TIE_BUCKET.as_millis() <= BACKOFF_FLOOR.as_millis());

/// Milliseconds from `epoch` to `now`; zero if `now` is earlier.
pub(crate) fn millis_since(epoch: Instant, now: Instant) -> u64 {
    u64::try_from(now.saturating_duration_since(epoch).as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
