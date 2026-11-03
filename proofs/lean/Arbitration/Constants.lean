-- `troll_le_bound`, `expected_useful_le` and `rearmed_root_goes_behind` are
-- what the side conditions below are for.
import Arbitration.Estimators
import Arbitration.Dynamics
import Arbitration.DueOrder

/-!
# Arbitration.Constants: the estimators' constants, the Q16 weight of a gap,
# and the side conditions the lemmas need, checked on exactly these values

The story, in plain words. A time-decayed estimate weighs a new sample by
`1 - 2^(-gap / H)`: the longer since the last sample, the more the new one
counts. Rust computes that weight in whole numbers out of 65536 from a table of
64 steps per half-life, halving per whole half-life, and linearly for a gap
under one step. Each estimate is one 64-bit word: 40 bits of time above 24
bits of value.

What must never happen:
* the table is not the floor of `65536 · 2^(-i/64)`, or it does not fall step
  by step, or it jumps up where one half-life wraps into the next
  (`table_is_floor`, `table_decreasing`, `table_wraps`);
* a weight exceeds one, so an update could pass its sample
  (`weight_le_one`), or a long silence fails to start the estimate over
  (`weight_full_after_silence`), or a gap of zero (a clock that went
  backwards) moves the estimate (`weight_zero_gap`);
* a packed word overflows: value and time do not fill exactly 64 bits, a
  share of one does not fit the value field, a Q16 product or the expected-time
  read does not fit 64 bits, or the clock wraps within decades
  (`packed_word_fits`, `ppm_fits_value`, `troll_product_fits`,
  `expected_useful_fits`, `timestamp_lasts`);
* the tie-break bucket is wider than the backoff floor, which
  `rearmed_root_goes_behind` needs (`tie_bucket_le_backoff_floor`);
* the useful floor is not `1/64`, which `expected_useful_le` needs
  (`useful_floor_is_one_64th`);
* occupancy's half-life on its tick is not 88 to 89 ticks, 176 to 178 s
  (`occupancy_half_life_ticks`).

Rust anchors: the constants in `crates/p2p/src/sync/arbitration/mod.rs`,
`weight_q16` in `ewma.rs`. `Conformance.lean` emits `contract` and
`proofs/tests/lean_conformance.rs` asserts the Rust constants format to the
same strings.
-/

namespace Arbitration

/-! ## The constants -/

-- Bits of a packed word holding the value.
def valueBits : Nat := 24
-- Bits holding the time of the last sample.
def timeBits : Nat := 40
-- A weight of one, in Q16.
def weightOne : Nat := 65536
-- `⌊65536 · ln 2⌋`: the weight per half-life of a gap under one table step.
def ln2Q16 : Nat := 45426
-- Half-life of the per-provider estimators, in ms.
def providerHalfLifeMs : Nat := 60000
-- Per-root estimators weigh an attempt one part in `2^rootShift`.
def rootShift : Nat := 1
-- Occupancy weighs a tick one part in `2^occupancyShift`.
def occupancyShift : Nat := 7
-- Occupancy's tick, in ms.
def occupancyTickMs : Nat := 2000
-- Width of a due-time bucket, in ms.
def tieBucketMs : Nat := 1000
-- The per-root backoff floor (`PENDING_RETRY_BASE`), in ms.
def backoffFloorMs : Nat := 2000
-- Unit of the latency and cost estimates, in µs.
def latencyUnitMicros : Nat := 4
-- A share of one, in parts per million.
def scalePpm : Nat := 1000000
-- Floor of the useful share in the expected-time read.
def usefulFloorPpm : Nat := 15625

-- `⌊65536 · 2^(-i/64)⌋` for `i < 64`.
def weightTable : List Nat :=
  [65536, 64830, 64131, 63440, 62757, 62081, 61412, 60751, 60096, 59449, 58809, 58175, 57548,
   56928, 56315, 55709, 55108, 54515, 53928, 53347, 52772, 52204, 51641, 51085, 50535, 49990,
   49452, 48919, 48392, 47871, 47355, 46845, 46340, 45841, 45347, 44859, 44376, 43898, 43425,
   42957, 42494, 42037, 41584, 41136, 40693, 40254, 39821, 39392, 38967, 38548, 38132, 37722,
   37315, 36913, 36516, 36122, 35733, 35348, 34968, 34591, 34218, 33850, 33485, 33124]

-- Step `i` of the table; zero past its end, which Rust's mask never reaches.
def tableAt (i : Nat) : Nat := weightTable.getD i 0

/-! ## The weight of a gap -/

-- `weightQ16`: `1 - 2^(-gap / h)` in Q16, as Rust's `weight_q16` computes it.
def weightQ16 (gap h : Nat) : Nat :=
  -- No half-life: every sample is taken as is.
  if h = 0 then weightOne
  -- Past 16 half-lives the kept share rounds to nothing.
  else if gap / h > 16 then weightOne
  -- Under one table step: linear in the gap, capped at the first step.
  else if gap / h = 0 ∧ gap % h * 64 / h = 0 then min (gap * ln2Q16 / h) (weightOne - tableAt 1)
  -- Otherwise: the step's kept share, halved per whole half-life.
  else weightOne - tableAt (gap % h * 64 / h % 64) / 2 ^ (gap / h)

-- Promise: a weight never exceeds one, so `troll` stays between its inputs.
theorem weight_le_one (gap h : Nat) : weightQ16 gap h ≤ weightOne := by
  -- Open the weight and take each branch.
  unfold weightQ16
  split
  -- No half-life: exactly one.
  · exact Nat.le_refl _
  split
  -- A long gap: exactly one.
  · exact Nat.le_refl _
  split
  -- The linear branch is capped below one.
  · exact Nat.le_trans (Nat.min_le_right _ _) (Nat.sub_le _ _)
  -- A table branch subtracts from one.
  · exact Nat.sub_le _ _

-- Promise: after more than 16 half-lives of silence the next sample is taken
-- as is: the estimate starts over without a staleness rule.
theorem weight_full_after_silence (gap h : Nat) (hh : 0 < h) (hg : 17 * h ≤ gap) :
    weightQ16 gap h = weightOne := by
  -- The gap spans at least 17 half-lives.
  have : 17 ≤ gap / h := (Nat.le_div_iff_mul_le hh).2 (by omega)
  -- So the second branch is taken.
  unfold weightQ16
  simp only [show ¬ h = 0 by omega, show gap / h > 16 by omega, ite_true, ite_false]

-- Promise: a zero gap, which is how a clock that went backwards reads, weighs
-- the sample at nothing: the estimate stays where it was.
theorem weight_zero_gap (h : Nat) (hh : 0 < h) : weightQ16 0 h = 0 := by
  -- Every quotient of zero is zero, so the linear branch gives zero.
  unfold weightQ16
  simp [show ¬ h = 0 by omega]

/-! ## The table, checked entry by entry -/

-- Promise: the table has 64 steps and starts at one.
theorem table_shape : weightTable.length = 64 ∧ tableAt 0 = weightOne := by decide

-- Promise: each step is the floor of `65536 · 2^(-i/64)`:
-- `t^64 · 2^i ≤ 2^1024 < (t + 1)^64 · 2^i`.
theorem table_is_floor :
    (List.range 64).all (fun i =>
      tableAt i ^ 64 * 2 ^ i ≤ 2 ^ 1024 && 2 ^ 1024 < (tableAt i + 1) ^ 64 * 2 ^ i) = true := by
  -- 1100-bit numbers: the kernel's arbitrary-precision arithmetic checks them.
  decide +kernel

-- Promise: the kept share falls with every step, so the weight never falls as
-- a gap grows within one half-life.
theorem table_decreasing :
    (List.range 63).all (fun i => tableAt (i + 1) < tableAt i) = true := by
  decide

-- Promise: the last step keeps at least half, so moving into the next
-- half-life (step 0, halved once more) never raises the kept share.
theorem table_wraps : tableAt 0 / 2 ≤ tableAt 63 := by decide

/-! ## Side conditions, on exactly these values -/

-- Promise: value and time fill one 64-bit word exactly.
theorem packed_word_fits : valueBits + timeBits = 64 := by decide

-- Promise: a share of one fits the 24-bit value field.
theorem ppm_fits_value : scalePpm < 2 ^ valueBits := by decide

-- Promise: a value times a weight fits 64 bits (Rust widens to 128 anyway).
theorem troll_product_fits : 2 ^ valueBits * weightOne < 2 ^ 64 := by decide

-- Promise: the largest cost, in µs, times a share of one fits 64 bits.
theorem expected_useful_fits : 2 ^ valueBits * latencyUnitMicros * scalePpm < 2 ^ 64 := by
  decide

-- Promise: 40 bits of milliseconds last more than 34 years.
theorem timestamp_lasts : 34 * 365 * 24 * 3600 * 1000 < 2 ^ timeBits := by decide

-- Promise: the tie-break bucket is at most the backoff floor, as
-- `rearmed_root_goes_behind` needs.
theorem tie_bucket_le_backoff_floor : 0 < tieBucketMs ∧ tieBucketMs ≤ backoffFloorMs := by
  decide

-- Promise: the useful floor is one 64th of a share of one, as
-- `expected_useful_le` needs.
theorem useful_floor_is_one_64th : usefulFloorPpm * 64 = scalePpm := by decide

-- Promise: `(1 - 2^-7)^88 > ½ > (1 - 2^-7)^89`: occupancy halves in 88 to 89
-- ticks, 176 to 178 s on the 2 s tick.
theorem occupancy_half_life_ticks :
    (2 ^ occupancyShift) ^ 88 < 2 * (2 ^ occupancyShift - 1) ^ 88
    ∧ 2 * (2 ^ occupancyShift - 1) ^ 89 < (2 ^ occupancyShift) ^ 89
    ∧ 88 * occupancyTickMs = 176000 := by
  decide

/-! ## The contract the Rust constants must match -/

-- `NAME=value` for every constant, in the order `lean_conformance.rs` lists them.
def contract : List String :=
  [ "VALUE_BITS=" ++ toString valueBits
  , "TIME_BITS=" ++ toString timeBits
  , "WEIGHT_ONE=" ++ toString weightOne
  , "LN2_Q16=" ++ toString ln2Q16
  , "PROVIDER_HALF_LIFE_MS=" ++ toString providerHalfLifeMs
  , "ROOT_SHIFT=" ++ toString rootShift
  , "OCCUPANCY_SHIFT=" ++ toString occupancyShift
  , "OCCUPANCY_TICK_MS=" ++ toString occupancyTickMs
  , "TIE_BUCKET_MS=" ++ toString tieBucketMs
  , "BACKOFF_FLOOR_MS=" ++ toString backoffFloorMs
  , "LATENCY_UNIT_MICROS=" ++ toString latencyUnitMicros
  , "SCALE_PPM=" ++ toString scalePpm
  , "USEFUL_FLOOR_PPM=" ++ toString usefulFloorPpm
  , "WEIGHT_TABLE=" ++ ",".intercalate (weightTable.map toString) ]

end Arbitration
