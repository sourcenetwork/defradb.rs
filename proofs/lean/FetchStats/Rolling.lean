/-!
# FetchStats.Rolling: per-peer fetch averages stay in range and never overflow

The story, in plain words. Each time we fetch blocks from a peer, we record how
it went: did it succeed, time out or come back partial, and how fast the bytes
arrived. For each peer we keep a rolling average of each of those, where every
new fetch counts for one eighth and the old average for the rest. We also keep
plain totals (fetches, bytes) that only ever grow.

What must never happen:
* a success, timeout or partial ratio leaves [0, 1] (`roll_ratio_stays_a_ratio`);
* a rolling average grows past the largest sample it was fed, so it can never
  overflow the integer that holds it (`roll_le_bound`, `rolling_over_samples`);
* a total wraps around to a small number (`bump_never_overflows`, `bump_grows`).

Rust anchors: `PeerCounters::record` and `roll` in
`crates/p2p/src/iroh/fetch_stats.rs`. Ratios are held in parts per million, so
a ratio of 1 is `scale`. Every value is a `u64`; `Nat` here never wraps, and the
theorems show the Rust values never need to.
-/

namespace FetchStats

-- A ratio of 1, in parts per million.
def scale : Nat := 1000000

-- A new sample counts for one part in `weight` of the average.
def weight : Nat := 8

-- The rolling average after one more sample: drop an eighth of the old average,
-- add an eighth of the sample.
def roll (avg sample : Nat) : Nat := avg - avg / weight + sample / weight

-- The first sample is taken as is; later ones roll into the average.
def record (count avg sample : Nat) : Nat :=
  -- No fetch recorded yet: the average is the sample itself;
  if count = 0 then sample
  -- otherwise the sample rolls in.
  else roll avg sample

-- Promise: if the average and the sample are both at most `bound`, so is the new
-- average.
theorem roll_le_bound (avg sample bound : Nat)
    -- Given the old average is within the bound,
    (havg : avg ≤ bound)
    -- and so is the new sample,
    (hs : sample ≤ bound) :
    -- the rolled average is too.
    roll avg sample ≤ bound := by
  -- Unfold the average and the weight; the rest is arithmetic with eighths.
  simp only [roll, weight]
  omega

-- Promise: recording a fetch keeps the average within any bound the samples obey.
theorem record_le_bound (count avg sample bound : Nat)
    -- Given the old average and the new sample are within the bound,
    (havg : avg ≤ bound) (hs : sample ≤ bound) :
    -- the recorded average is too.
    record count avg sample ≤ bound := by
  -- Split on whether this is the first fetch.
  unfold record
  split
  -- First fetch: the average is the sample, which is within the bound.
  · exact hs
  -- Later fetch: it is a roll, covered above.
  · exact roll_le_bound avg sample bound havg hs

-- A fetch either counts as a hit for a ratio (scale) or not (zero).
def hitSample (hit : Bool) : Nat := if hit then scale else 0

-- Promise: a ratio fed only hits and misses stays between 0 and 1.
theorem roll_ratio_stays_a_ratio (count avg : Nat) (hit : Bool)
    -- Given the ratio is at most 1,
    (havg : avg ≤ scale) :
    -- after one more fetch it still is (it is a `Nat`, so it is never below 0).
    record count avg (hitSample hit) ≤ scale := by
  -- A hit or a miss is at most `scale`.
  have hs : hitSample hit ≤ scale := by cases hit <;> simp [hitSample]
  -- Then it is the bound promise with `scale` as the bound.
  exact record_le_bound count avg (hitSample hit) scale havg hs

-- Promise: however many fetches roll in, the average stays within any bound that
-- every sample obeys. With the bound set to the largest `u64`, the average never
-- overflows.
theorem rolling_over_samples (samples : List Nat) (avg bound : Nat)
    -- Given the starting average is within the bound,
    (havg : avg ≤ bound)
    -- and every sample is too,
    (hs : ∀ s ∈ samples, s ≤ bound) :
    -- rolling them all in, one at a time, ends within the bound.
    samples.foldl roll avg ≤ bound := by
  -- Induct on the samples, carrying the average along.
  induction samples generalizing avg with
  -- No samples: the average is unchanged.
  | nil => simpa using havg
  -- One sample then the rest: roll the first in, then use the rest's promise.
  | cons s rest ih =>
    -- The rest's samples are all within the bound too.
    apply ih
    -- The first roll stays within the bound.
    · exact roll_le_bound avg s bound havg (hs s (by simp))
    -- Every remaining sample was in the original list.
    · intro x hx
      exact hs x (by simp [hx])

-- The largest value a `u64` holds.
def maxU64 : Nat := 2 ^ 64 - 1

-- Add to a total without going past the largest `u64`, as `saturating_add` does.
def bump (total add : Nat) : Nat := min (total + add) maxU64

-- Promise: a total never goes past the largest `u64`, so it never wraps.
theorem bump_never_overflows (total add : Nat) : bump total add ≤ maxU64 := by
  -- The minimum of anything and `maxU64` is at most `maxU64`.
  exact Nat.min_le_right _ _

-- Promise: a total within range never goes down when added to.
theorem bump_grows (total add : Nat)
    -- Given the total is a valid `u64`,
    (h : total ≤ maxU64) :
    -- adding to it never makes it smaller.
    total ≤ bump total add := by
  -- Both sides of the minimum are at least `total`.
  simp only [bump]
  omega

end FetchStats
