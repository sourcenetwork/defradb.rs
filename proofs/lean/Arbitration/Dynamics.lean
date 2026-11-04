-- `roll`, `div_add_le`, `div_mono` and the one-step bounds come from
-- Arbitration.Estimators.
import Arbitration.Estimators

/-!
# Arbitration.Dynamics: the averages converge without overshoot, decay
# geometrically, and the expected-time read stays bounded

The story, in plain words. Feed an average the same sample again and again
(occupancy does this over the ticks it missed) and it climbs toward it one
roll at a time, never past it. After a step change the distance left shrinks
by a fixed share per sample, down to a rounding dead zone of `w - 1` units.
The expected time to a useful fetch, `cost / max(useful, 1/64)`, is at most 64
fetches' cost.

What must never happen:
* under a steady input the average overshoots or turns back
  (`iter_monotone_up`, `iter_stays_below`);
* the error after `k` samples of a step exceeds `(1 - 1/w)^k` of the step plus
  the dead zone (`decay_step_up`, `decay_step_down`, `decay_step`,
  `decay_geometric`);
* the expected time to a useful fetch is unbounded for a provider that is
  never useful (`expected_useful_le`).

Rust anchors: `advance` in `crates/p2p/src/sync/arbitration/occupancy.rs` (the
steady input over missed ticks), `RootEstimates::observe` in `root.rs` (weight
`1/2`), `expected_useful` in `provider.rs`. Nothing reads an average to set a
deadline, a backoff or a share, so no lemma here models one.
-/

namespace Arbitration

/-! ## Steady input: monotone, no overshoot -/

-- `k` rolls of the same sample `y`, as occupancy does over `k` missed ticks.
def iter (w y : Nat) : Nat → Nat → Nat
  -- No roll: the average as it was.
  | x, 0 => x
  -- One roll, then the rest.
  | x, k + 1 => iter w y (roll w x y) k

-- Promise: from below a steady input, the average never passes it.
theorem iter_stays_below {w y : Nat} (hw : 0 < w) :
    ∀ (k x : Nat), x ≤ y → iter w y x k ≤ y := by
  -- Induct on the number of rolls.
  intro k
  induction k with
  -- No roll: the start is below the input.
  | zero => intro x hx; exact hx
  -- One roll stays below the input, then the rest do.
  | succ k ih => intro x hx; exact ih _ (roll_between_up hw hx).2

-- Promise: from below a steady input, the average only rises.
theorem iter_monotone_up {w y : Nat} (hw : 0 < w) :
    ∀ (k x : Nat), x ≤ y → iter w y x k ≤ iter w y x (k + 1) := by
  -- Induct on the number of rolls.
  intro k
  induction k with
  -- One roll from below rises.
  | zero => intro x hx; exact (roll_between_up hw hx).1
  -- After one roll the average is still below the input.
  | succ k ih =>
    intro x hx
    -- So the claim for the rest applies.
    exact ih _ (roll_between_up hw hx).2

/-! ## Step change: geometric decay -/

-- Promise (one step, rising): the distance to a steady input shrinks by at
-- least `1/w` of itself, rounded down.
theorem decay_step_up {w x y : Nat} (hw : 0 < w) (h : x ≤ y) :
    y - roll w x y ≤ (y - x) - (y - x) / w := by
  -- Open the update.
  unfold roll
  -- The two shares together are at most the share of the sum.
  have s := @div_add_le (y - x) x w hw
  -- The sum is the input.
  rw [show y - x + x = y by omega] at s
  -- A quotient never exceeds its dividend.
  have := Nat.div_le_self x w
  -- Nor for the distance.
  have := Nat.div_le_self (y - x) w
  -- The roll stays below the input.
  have := (roll_between_up hw h).2
  -- In unfolded form.
  unfold roll at this
  -- The rest is linear arithmetic.
  omega

-- Promise (one step, falling): the same from above.
theorem decay_step_down {w x y : Nat} (hw : 0 < w) (h : y ≤ x) :
    roll w x y - y ≤ (x - y) - (x - y) / w := by
  -- Open the update.
  unfold roll
  -- The two shares together are at most the share of the sum.
  have s := @div_add_le (x - y) y w hw
  -- The sum is the old average.
  rw [show x - y + y = x by omega] at s
  -- A quotient never exceeds its dividend.
  have := Nat.div_le_self y w
  -- Nor for the distance.
  have := Nat.div_le_self (x - y) w
  -- The roll stays above the input.
  have := (roll_between_down hw h).1
  -- In unfolded form.
  unfold roll at this
  -- The rest is linear arithmetic.
  omega

-- The one-step bound in scaled form: `w e' ≤ (w-1) e + (w-1)`.
theorem decay_step {w e e' : Nat} (hw : 0 < w) (h : e' ≤ e - e / w) :
    w * e' ≤ (w - 1) * e + (w - 1) := by
  -- Split `e` into quotient and remainder by `w`.
  have dm := Nat.div_add_mod e w
  -- The remainder is below `w`.
  have ml := Nat.mod_lt e hw
  -- Scale the hypothesis by `w`.
  have m1 : w * e' ≤ w * (e - e / w) := Nat.mul_le_mul_left w h
  -- Distribute over the subtraction.
  have m2 : w * (e - e / w) = w * e - w * (e / w) := Nat.mul_sub_left_distrib w e (e / w)
  -- `(w - 1) e = w e - e`.
  have m3 : (w - 1) * e = w * e - e := Nat.sub_one_mul w e
  -- `e ≤ w e` for a positive weight.
  have m4 : e ≤ w * e := Nat.le_mul_of_pos_left e hw
  -- `w (e / w) ≤ w e`.
  have m5 : w * (e / w) ≤ w * e := Nat.mul_le_mul_left w (Nat.div_le_self e w)
  -- The rest is linear arithmetic.
  omega

-- Promise: any error sequence that obeys the one-step bound decays
-- geometrically to the dead zone: `w^k e_k ≤ (w-1)^k e_0 + (w-1) w^k`, i.e.
-- `e_k ≤ (1 - 1/w)^k e_0 + (w - 1)`. With `w = 2^s` the half-life is
-- `ln 2 / -ln(1 - 2^-s)` samples.
theorem decay_geometric {w : Nat} (hw : 0 < w) (e : Nat → Nat)
    (step : ∀ k, w * e (k + 1) ≤ (w - 1) * e k + (w - 1)) :
    ∀ k, w ^ k * e k ≤ (w - 1) ^ k * e 0 + (w - 1) * w ^ k := by
  -- Induct on the number of samples.
  intro k
  induction k with
  -- No sample: both sides start from `e 0`.
  | zero => simp
  -- One more sample.
  | succ k ih =>
    -- The step bound, scaled by `w^k`.
    have s := Nat.mul_le_mul_left (w ^ k) (step k)
    -- The claim so far, scaled by `w - 1`.
    have i := Nat.mul_le_mul_left (w - 1) ih
    -- Regroup the left side.
    have p1 : w ^ (k + 1) * e (k + 1) = w ^ k * (w * e (k + 1)) := by
      rw [Nat.pow_succ, Nat.mul_assoc]
    -- Distribute the scaled step bound.
    have p2 : w ^ k * ((w - 1) * e k + (w - 1)) = (w - 1) * (w ^ k * e k) + (w - 1) * w ^ k := by
      simp only [Nat.mul_add, Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc]
    -- Distribute the scaled claim.
    have p3 : (w - 1) * ((w - 1) ^ k * e 0 + (w - 1) * w ^ k)
        = (w - 1) ^ (k + 1) * e 0 + (w - 1) * ((w - 1) * w ^ k) := by
      rw [Nat.mul_add, ← Nat.mul_assoc, Nat.pow_succ, Nat.mul_comm ((w - 1) ^ k) (w - 1)]
    -- The two dead-zone terms add up to the next one.
    have p4 : (w - 1) * ((w - 1) * w ^ k) + (w - 1) * w ^ k = (w - 1) * w ^ (k + 1) := by
      -- Factor out `w - 1` and expand the power.
      rw [← Nat.mul_add, Nat.pow_succ]
      -- `(w - 1) w^k + w^k = w · w^k`.
      have h1 : (w - 1) * w ^ k + w ^ k = (w - 1 + 1) * w ^ k := (Nat.succ_mul (w - 1) (w ^ k)).symm
      -- `w - 1 + 1 = w` for a positive weight.
      have h2 : w - 1 + 1 = w := by omega
      -- Put it together.
      rw [h1, h2, Nat.mul_comm w (w ^ k)]
    -- The rest is linear arithmetic.
    omega

/-! ## The expected time to a useful fetch -/

-- `expectedUseful`: a fetch's cost over the useful share, the share floored at
-- `floor` parts in `scale`.
def expectedUseful (scale floor cost useful : Nat) : Nat := cost * scale / max useful floor

-- Promise: with the floor at `1/64` of the scale, a provider that is never
-- useful costs at most 64 fetches, so the read never overflows a bounded cost.
theorem expected_useful_le (scale floor cost useful : Nat)
    (h64 : floor * 64 = scale) : expectedUseful scale floor cost useful ≤ cost * 64 := by
  -- Open the read.
  unfold expectedUseful
  -- The divisor is at least the floor.
  have hm : floor ≤ max useful floor := Nat.le_max_right _ _
  -- A quotient is at most `cost · 64` when the dividend is at most the divisor
  -- times `cost · 64`.
  apply Nat.div_le_of_le_mul
  -- `cost · scale = floor · (cost · 64)`.
  rw [← h64, Nat.mul_left_comm cost floor 64]
  -- And `floor ≤ max useful floor`, times `cost · 64`.
  exact Nat.mul_le_mul_right _ hm

end Arbitration
