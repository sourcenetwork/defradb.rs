/-!
# Arbitration.Estimators: the fixed-point averages stay in range and start
# from the first sample

The story, in plain words. A node keeps a few running averages about DAG
fetching: per provider, how long a useful fetch takes, how often a fetch is
useful and what any fetch costs; per pending root, how long its attempts run
and whether they advance it; per source peer, how much of the pending map it
holds. Each average is a whole number. Two updates move it:

* `roll`, per sample: a new sample counts for one part in `w` (`w = 2^shift`)
  and the old average for the rest, `avg - avg / w + sample / w`. The per-root
  averages use `w = 2`, the per-peer occupancy `w = 128` once per tick.
* `troll`, per unit of time: the average moves a `w / 65536` share of the way to
  the sample, where `w` grows with the time since the last sample. The
  per-provider averages use it, so their half-life is in seconds on every link.

The first sample is taken as is. Nothing reads these averages yet except the
sync status.

What must never happen:
* an average leaves the range of what it was fed, so it could overflow
  (`roll_between_up`, `roll_between_down`, `roll_le_bound`, `troll_between_up`,
  `troll_between_down`, `troll_le_bound`);
* the first sample is mixed with a made-up zero (`record_first`, `troll_full`);
* a time-weighted sample removes less than its share of the distance
  (`troll_decay_up`, `troll_decay_down`).

How the averages behave over many samples is in `Arbitration.Dynamics`.

Rust anchors: `roll`, `troll` and `record_word` in
`crates/p2p/src/sync/arbitration/ewma.rs`, which `RootEstimates::observe`
(`root.rs`), `advance` (`occupancy.rs`) and `ProviderEstimates` (`provider.rs`)
call.
Every Rust value is a `u64` (products in `u128`); `Nat` here never wraps, and
the theorems show the Rust values never need to.
-/

namespace Arbitration

/-! ## Arithmetic helpers -/

-- Division by a positive weight never reverses an order.
theorem div_mono {a b c : Nat} (hc : 0 < c) (h : a ≤ b) : a / c ≤ b / c :=
  -- `a / c * c ≤ a ≤ b`, so `a / c` is at most `b / c`.
  (Nat.le_div_iff_mul_le hc).2 (Nat.le_trans (Nat.div_mul_le_self a c) h)

-- The part of an average a roll keeps, `z - z / w`, never shrinks as `z` grows.
theorem keep_mono {w a b : Nat} (hw : 0 < w) (h : a ≤ b) : a - a / w ≤ b - b / w := by
  -- Write `b` as `a + d` and add one unit at a time.
  obtain ⟨d, rfl⟩ := Nat.exists_eq_add_of_le h
  -- Induct on the number of units added.
  induction d with
  -- No unit added: both sides are equal.
  | zero => simp
  -- One more unit on top of `a + d`.
  | succ d ih =>
    -- The claim for `a + d`.
    have ih := ih (Nat.le_add_right a d)
    -- One more unit adds at most one to the quotient.
    have h1 : (a + d + 1) / w ≤ (a + d) / w + 1 := by
      -- `a + d + 1 ≤ a + d + w` because `w ≥ 1`.
      have := div_mono hw (show a + d + 1 ≤ a + d + w by omega)
      -- And `(a + d + w) / w = (a + d) / w + 1`.
      rw [Nat.add_div_right (a + d) hw] at this
      -- That is the bound.
      exact this
    -- A quotient never exceeds its dividend, before and after.
    have h2 := Nat.div_le_self (a + d) w
    -- The same for the larger dividend.
    have h3 := Nat.div_le_self (a + d + 1) w
    -- Regroup the sum to match the hypotheses.
    rw [show a + (d + 1) = a + d + 1 by omega]
    -- The rest is linear arithmetic.
    omega

-- Floors add at most up: `a / c + b / c ≤ (a + b) / c`.
theorem div_add_le {a b c : Nat} (hc : 0 < c) : a / c + b / c ≤ (a + b) / c := by
  -- It is enough that the left side times `c` is at most `a + b`.
  apply (Nat.le_div_iff_mul_le hc).2
  -- Distribute the product over the sum.
  rw [Nat.add_mul]
  -- `a / c * c ≤ a`.
  have := Nat.div_mul_le_self a c
  -- `b / c * c ≤ b`.
  have := Nat.div_mul_le_self b c
  -- Add the two.
  omega

/-! ## The per-sample update -/

-- `roll`: one more sample, weight `1/w`.
def roll (w avg sample : Nat) : Nat := avg - avg / w + sample / w

-- `record`: the first sample is taken as is, later ones roll in.
def record (w count avg sample : Nat) : Nat :=
  -- No sample yet: the average is the sample itself; otherwise it rolls in.
  if count = 0 then sample else roll w avg sample

-- Promise: the first sample is the estimate, not a blend with zero.
theorem record_first (w avg sample : Nat) : record w 0 avg sample = sample := by
  -- The first branch of `record` is taken.
  simp [record]

-- Promise: fed a sample at or above the average, the new average lies between
-- them.
theorem roll_between_up {w avg s : Nat} (hw : 0 < w) (h : avg ≤ s) :
    avg ≤ roll w avg s ∧ roll w avg s ≤ s := by
  -- Open the update.
  unfold roll
  -- The kept part grows with its argument.
  have k := keep_mono hw h
  -- So does the added part.
  have d := div_mono hw h
  -- A quotient never exceeds its dividend.
  have := Nat.div_le_self avg w
  -- Nor for the sample.
  have := Nat.div_le_self s w
  -- The rest is linear arithmetic.
  omega

-- Promise: fed a sample at or below the average, the new average lies between
-- them.
theorem roll_between_down {w avg s : Nat} (hw : 0 < w) (h : s ≤ avg) :
    s ≤ roll w avg s ∧ roll w avg s ≤ avg := by
  -- Open the update.
  unfold roll
  -- The kept part grows with its argument.
  have k := keep_mono hw h
  -- So does the added part.
  have d := div_mono hw h
  -- A quotient never exceeds its dividend.
  have := Nat.div_le_self avg w
  -- Nor for the sample.
  have := Nat.div_le_self s w
  -- The rest is linear arithmetic.
  omega

-- Promise: an average and a sample within a bound stay within it, so with the
-- bound at `2^64 - 1` the `u64` never overflows.
theorem roll_le_bound {w avg s bound : Nat} (hw : 0 < w) (ha : avg ≤ bound) (hs : s ≤ bound) :
    roll w avg s ≤ bound := by
  -- Either the sample is above the average or below it.
  rcases Nat.le_total avg s with h | h
  -- Above: the new average is at most the sample.
  · exact Nat.le_trans (roll_between_up hw h).2 hs
  -- Below: the new average is at most the old one.
  · exact Nat.le_trans (roll_between_down hw h).2 ha

/-! ## The time-weighted update -/

-- `troll`: move `w/65536` of the way from `v` to `x`, rounding toward `v`.
def troll (v x w : Nat) : Nat :=
  -- Rising: add the share of the distance; falling: subtract it.
  if v ≤ x then v + (x - v) * w / 65536 else v - (v - x) * w / 65536

-- A Q16 share of a distance is at most the distance.
theorem share_le (d w : Nat) (hw : w ≤ 65536) : d * w / 65536 ≤ d := by
  -- It is enough that `d * w ≤ d * 65536`.
  apply Nat.div_le_of_le_mul
  -- Put the factors in the same order.
  rw [Nat.mul_comm 65536 d]
  -- A weight of at most one only shrinks the distance.
  exact Nat.mul_le_mul_left _ hw

-- Promise: the first sample (or the first after a long silence) has full
-- weight, so the estimate becomes the sample itself.
theorem troll_full (v x : Nat) : troll v x 65536 = x := by
  -- Open the update.
  unfold troll
  -- Rising or falling.
  split
  -- Rising: the whole distance is added.
  · rw [Nat.mul_div_cancel _ (by decide : 0 < 65536)]
    -- `v + (x - v) = x`.
    omega
  -- Falling: the whole distance is subtracted.
  · rw [Nat.mul_div_cancel _ (by decide : 0 < 65536)]
    -- `v - (v - x) = x`.
    omega

-- Promise: for any weight in `[0, 65536]`, a rising estimate lies between the
-- old one and the sample.
theorem troll_between_up {v x w : Nat} (hvx : v ≤ x) (hw : w ≤ 65536) :
    v ≤ troll v x w ∧ troll v x w ≤ x := by
  -- Open the update and take the rising branch.
  unfold troll
  simp only [hvx, ite_true]
  -- The share added is at most the distance.
  have := share_le (x - v) w hw
  -- The rest is linear arithmetic.
  omega

-- Promise: the falling case of the same.
theorem troll_between_down {v x w : Nat} (hxv : x < v) (hw : w ≤ 65536) :
    x ≤ troll v x w ∧ troll v x w ≤ v := by
  -- Open the update and take the falling branch.
  unfold troll
  simp only [show ¬ v ≤ x by omega, ite_false]
  -- The share subtracted is at most the distance.
  have := share_le (v - x) w hw
  -- The rest is linear arithmetic.
  omega

-- Promise: an estimate and a sample within a bound stay within it, so a
-- 24-bit value field never overflows.
theorem troll_le_bound {v x w bound : Nat} (hw : w ≤ 65536) (hv : v ≤ bound) (hx : x ≤ bound) :
    troll v x w ≤ bound := by
  -- Either the sample is above the estimate or below it.
  rcases Nat.lt_or_ge v (x + 1) with h | h
  -- Above or equal: the new estimate is at most the sample.
  · exact Nat.le_trans (troll_between_up (by omega) hw).2 hx
  -- Below: the new estimate is at most the old one.
  · exact Nat.le_trans (troll_between_down (by omega) hw).2 hv

-- Promise (one step, rising): the remaining distance is at most the old one
-- less its `w/65536` share, plus one unit of rounding. Chained over samples
-- this is `e_k ≲ Π (1 - w_i) e_0 = 2^(-(t_k - t_0)/H) e_0`: a half-life of `H`
-- seconds whatever the sample rate.
theorem troll_decay_up {v x w : Nat} (hvx : v ≤ x) (hw : w ≤ 65536) :
    65536 * (x - troll v x w) ≤ (65536 - w) * (x - v) + 65536 := by
  -- Open the update and take the rising branch.
  unfold troll
  simp only [hvx, ite_true]
  -- Split the product into quotient and remainder.
  have dm := Nat.div_add_mod ((x - v) * w) 65536
  -- The remainder is below 65536.
  have ml := Nat.mod_lt ((x - v) * w) (show 65536 > 0 by decide)
  -- The share is at most the distance.
  have d := share_le (x - v) w hw
  -- Distribute 65536 over the remaining distance.
  have e1 : 65536 * (x - (v + (x - v) * w / 65536))
      = 65536 * (x - v) - 65536 * ((x - v) * w / 65536) := by
    -- The remaining distance is the old one less the share.
    rw [show x - (v + (x - v) * w / 65536) = (x - v) - (x - v) * w / 65536 by omega]
    -- Multiplication distributes over truncated subtraction.
    exact Nat.mul_sub_left_distrib _ _ _
  -- Distribute the kept weight over the old distance.
  have e2 : (65536 - w) * (x - v) = 65536 * (x - v) - w * (x - v) :=
    Nat.mul_sub_right_distrib _ _ _
  -- Products commute.
  have e3 : w * (x - v) = (x - v) * w := Nat.mul_comm _ _
  -- The weight is at most one.
  have e4 : w * (x - v) ≤ 65536 * (x - v) := Nat.mul_le_mul_right _ hw
  -- The rest is linear arithmetic.
  omega

-- Promise (one step, falling): the same bound when the sample is below.
theorem troll_decay_down {v x w : Nat} (hxv : x < v) (hw : w ≤ 65536) :
    65536 * (troll v x w - x) ≤ (65536 - w) * (v - x) + 65536 := by
  -- Open the update and take the falling branch.
  unfold troll
  simp only [show ¬ v ≤ x by omega, ite_false]
  -- Split the product into quotient and remainder.
  have dm := Nat.div_add_mod ((v - x) * w) 65536
  -- The remainder is below 65536.
  have ml := Nat.mod_lt ((v - x) * w) (show 65536 > 0 by decide)
  -- The share is at most the distance.
  have d := share_le (v - x) w hw
  -- Distribute 65536 over the remaining distance.
  have e1 : 65536 * (v - (v - x) * w / 65536 - x)
      = 65536 * (v - x) - 65536 * ((v - x) * w / 65536) := by
    -- The remaining distance is the old one less the share.
    rw [show v - (v - x) * w / 65536 - x = (v - x) - (v - x) * w / 65536 by omega]
    -- Multiplication distributes over truncated subtraction.
    exact Nat.mul_sub_left_distrib _ _ _
  -- Distribute the kept weight over the old distance.
  have e2 : (65536 - w) * (v - x) = 65536 * (v - x) - w * (v - x) :=
    Nat.mul_sub_right_distrib _ _ _
  -- Products commute.
  have e3 : w * (v - x) = (v - x) * w := Nat.mul_comm _ _
  -- The weight is at most one.
  have e4 : w * (v - x) ≤ 65536 * (v - x) := Nat.mul_le_mul_right _ hw
  -- The rest is linear arithmetic.
  omega

end Arbitration
