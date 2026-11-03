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
* `troll`, per unit of time: the average moves a `w / 2^32` share of the way to
  the sample, where `w` grows with the time since the last sample. The
  per-provider averages use it, so their half-life is in seconds on every link.
  The share's fraction is dithered, so a share below one unit still moves the
  average in proportion, on average, instead of freezing it.

The first sample is taken as is. Nothing reads these averages yet except the
sync status.

What must never happen:
* an average leaves the range of what it was fed, so it could overflow
  (`roll_between_up`, `roll_between_down`, `roll_le_bound`, `troll_between_up`,
  `troll_between_down`, `troll_le_bound`);
* the first sample is mixed with a made-up zero (`record_first`, `troll_full`);
* a time-weighted sample removes less than its share of the distance
  (`troll_decay_up`, `troll_decay_down`), or the dither biases the move away
  from its exact share (`dither_mean`).

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

-- A weight of one, in Q32.
def one : Nat := 4294967296

-- `troll`: move `w / one` of the way from `v` to `x`. The share's fraction is
-- rounded by the dither `r < one`: down, unless `r` makes up the rest.
def troll (v x w r : Nat) : Nat :=
  -- Rising: add the dithered share of the distance; falling: subtract it.
  if v ≤ x then v + ((x - v) * w + r) / one else v - ((v - x) * w + r) / one

-- A dithered share of a distance is at most the distance.
theorem share_le (d w r : Nat) (hw : w ≤ one) (hr : r < one) : (d * w + r) / one ≤ d := by
  -- It is enough that the dividend is below `(d + 1) · one`.
  apply Nat.le_of_lt_succ
  apply (Nat.div_lt_iff_lt_mul (by decide : 0 < one)).2
  -- A weight of at most one only shrinks the distance.
  have : d * w ≤ d * one := Nat.mul_le_mul_left _ hw
  -- `(d + 1) · one = d · one + one`, and `r < one`.
  rw [Nat.succ_mul]
  omega

-- Promise: the first sample (or the first after a long silence) has full
-- weight, so the estimate becomes the sample itself, whatever the dither.
theorem troll_full (v x r : Nat) (hr : r < one) : troll v x one r = x := by
  -- Open the update.
  unfold troll
  -- The whole distance moves: `(d · one + r) / one = d` for `r < one`.
  have q : ∀ d, (d * one + r) / one = d := fun d => by
    rw [Nat.add_comm, Nat.add_mul_div_right _ _ (by decide : 0 < one),
      Nat.div_eq_of_lt hr, Nat.zero_add]
  -- Rising or falling.
  split
  -- Rising: `v + (x - v) = x`.
  · rw [q]; omega
  -- Falling: `v - (v - x) = x`.
  · rw [q]; omega

-- Promise: for any weight in `[0, one]` and any dither, a rising estimate lies
-- between the old one and the sample.
theorem troll_between_up {v x w r : Nat} (hvx : v ≤ x) (hw : w ≤ one) (hr : r < one) :
    v ≤ troll v x w r ∧ troll v x w r ≤ x := by
  -- Open the update and take the rising branch.
  unfold troll
  simp only [hvx, ite_true]
  -- The share added is at most the distance.
  have := share_le (x - v) w r hw hr
  -- Name the share; the rest is linear arithmetic.
  generalize ((x - v) * w + r) / one = q at *
  omega

-- Promise: the falling case of the same.
theorem troll_between_down {v x w r : Nat} (hxv : x < v) (hw : w ≤ one) (hr : r < one) :
    x ≤ troll v x w r ∧ troll v x w r ≤ v := by
  -- Open the update and take the falling branch.
  unfold troll
  simp only [show ¬ v ≤ x by omega, ite_false]
  -- The share subtracted is at most the distance.
  have := share_le (v - x) w r hw hr
  -- Name the share; the rest is linear arithmetic.
  generalize ((v - x) * w + r) / one = q at *
  omega

-- Promise: an estimate and a sample within a bound stay within it, so a
-- 24-bit value field never overflows.
theorem troll_le_bound {v x w r bound : Nat} (hw : w ≤ one) (hr : r < one) (hv : v ≤ bound)
    (hx : x ≤ bound) : troll v x w r ≤ bound := by
  -- Either the sample is above the estimate or below it.
  rcases Nat.lt_or_ge v (x + 1) with h | h
  -- Above or equal: the new estimate is at most the sample.
  · exact Nat.le_trans (troll_between_up (by omega) hw hr).2 hx
  -- Below: the new estimate is at most the old one.
  · exact Nat.le_trans (troll_between_down (by omega) hw hr).2 hv

-- A dithered share is at least the exact share less one unit:
-- `one · ((d w + r) / one) + one ≥ d w + 1`.
theorem share_ge (d w r : Nat) : d * w + 1 ≤ one * ((d * w + r) / one) + one := by
  -- Split the dividend into quotient and remainder.
  have dm := Nat.div_add_mod (d * w + r) one
  -- The remainder is below one.
  have ml := Nat.mod_lt (d * w + r) (by decide : one > 0)
  -- The rest is linear arithmetic.
  omega

-- Promise (one step, rising): the remaining distance is at most the old one
-- less its `w/one` share, plus one unit of rounding. Chained over samples this
-- is `e_k ≲ Π (1 - w_i) e_0 = 2^(-(t_k - t_0)/H) e_0`: a half-life of `H`
-- seconds whatever the sample rate.
theorem troll_decay_up {v x w r : Nat} (hvx : v ≤ x) (hw : w ≤ one) (hr : r < one) :
    one * (x - troll v x w r) ≤ (one - w) * (x - v) + one := by
  -- The new estimate is within the distance.
  have b := (troll_between_up hvx hw hr).2
  -- Open the update and take the rising branch.
  unfold troll at b ⊢
  simp only [hvx, ite_true] at b ⊢
  -- The share is at least the exact share less one unit.
  have g := share_ge (x - v) w r
  -- Distribute `one` over the remaining distance.
  have e1 : one * (x - (v + ((x - v) * w + r) / one))
      = one * (x - v) - one * (((x - v) * w + r) / one) := by
    rw [show x - (v + ((x - v) * w + r) / one) = (x - v) - ((x - v) * w + r) / one by omega]
    exact Nat.mul_sub_left_distrib _ _ _
  -- Distribute the kept weight over the old distance.
  have e2 : (one - w) * (x - v) = one * (x - v) - w * (x - v) := Nat.mul_sub_right_distrib _ _ _
  -- Products commute.
  have e3 : w * (x - v) = (x - v) * w := Nat.mul_comm _ _
  -- The weight is at most one.
  have e4 : w * (x - v) ≤ one * (x - v) := Nat.mul_le_mul_right _ hw
  -- The rest is linear arithmetic.
  omega

-- Promise (one step, falling): the same bound when the sample is below.
theorem troll_decay_down {v x w r : Nat} (hxv : x < v) (hw : w ≤ one) (hr : r < one) :
    one * (troll v x w r - x) ≤ (one - w) * (v - x) + one := by
  -- The new estimate is within the distance.
  have b := (troll_between_down hxv hw hr).1
  -- Open the update and take the falling branch.
  unfold troll at b ⊢
  simp only [show ¬ v ≤ x by omega, ite_false] at b ⊢
  -- The share is at least the exact share less one unit.
  have g := share_ge (v - x) w r
  -- The share is at most the distance.
  have l := share_le (v - x) w r hw hr
  -- Distribute `one` over the remaining distance.
  have e1 : one * (v - ((v - x) * w + r) / one - x)
      = one * (v - x) - one * (((v - x) * w + r) / one) := by
    rw [show v - ((v - x) * w + r) / one - x = (v - x) - ((v - x) * w + r) / one by omega]
    exact Nat.mul_sub_left_distrib _ _ _
  -- Distribute the kept weight over the old distance.
  have e2 : (one - w) * (v - x) = one * (v - x) - w * (v - x) := Nat.mul_sub_right_distrib _ _ _
  -- Products commute.
  have e3 : w * (v - x) = (v - x) * w := Nat.mul_comm _ _
  -- The weight is at most one.
  have e4 : w * (v - x) ≤ one * (v - x) := Nat.mul_le_mul_right _ hw
  -- The rest is linear arithmetic.
  omega

/-! ## The dither moves by the exact share on average

A share below one unit, rounded down every time, would never move the
estimate. The dither rounds it up for a matching fraction of its values:
summed over every dither `r < n`, the moves add up to exactly the share's
numerator `a`, so on average each moves `a / n`, fraction included.
-/

-- The sum of `f r` over `r < n`.
def sumBelow (f : Nat → Nat) : Nat → Nat
  -- Nothing below zero;
  | 0 => 0
  -- one more term.
  | n + 1 => sumBelow f n + f n

-- Shifting the numerator by one trades the first term for one past the end.
theorem sum_shift (n a k : Nat) :
    sumBelow (fun r => (a + 1 + r) / n) k + a / n = sumBelow (fun r => (a + r) / n) k + (a + k) / n := by
  -- Induct on the number of terms.
  induction k with
  -- No terms: both sides are `a / n`.
  | zero => simp [sumBelow]
  -- One more term on each side.
  | succ k ih =>
    -- Open one step of each sum.
    simp only [sumBelow]
    -- The new left term is the right side's `(a + (k + 1)) / n`.
    rw [show a + 1 + k = a + (k + 1) by omega]
    -- The rest is linear arithmetic over the claim for `k`.
    omega

-- Promise (`dither_mean`): over every dither `r < n`, the rounded shares add up
-- to the numerator, `Σ_{r<n} ⌊(a + r) / n⌋ = a` (Hermite's identity), so a
-- uniform dither moves the estimate by the exact share on average.
theorem dither_mean (n a : Nat) (hn : 0 < n) : sumBelow (fun r => (a + r) / n) n = a := by
  -- Induct on the numerator.
  induction a with
  -- Zero: every term `r / n` with `r < n` is zero.
  | zero =>
    -- The partial sums up to any `k ≤ n` are zero.
    have z : ∀ k, k ≤ n → sumBelow (fun r => (0 + r) / n) k = 0 := by
      -- Induct on the number of terms.
      intro k
      induction k with
      -- No terms.
      | zero => intro _; rfl
      -- One more term, below `n`, so zero.
      | succ k ih =>
        intro hk
        simp only [sumBelow]
        rw [ih (by omega), Nat.div_eq_of_lt (by omega : 0 + k < n)]
    exact z n (Nat.le_refl n)
  -- One more: shift the sum by one term.
  | succ a ih =>
    -- The shifted sum, at `k = n`.
    have s := sum_shift n a n
    -- `(a + n) / n = a / n + 1`.
    rw [Nat.add_div_right a hn, ih] at s
    -- The rest is linear arithmetic.
    omega

end Arbitration
