/-!
# Positive counter guard — a P-counter never goes down

The story. A counter field holds a whole number. Every write to it is an
"increment by d". A positive counter (`pcounter`) is the kind that may only go
up. Its guard, `CounterField::op` in `crates/crdt/src/field/counter.rs`, looks
at each incoming `d` before anything is stored. If `d` is negative the guard
says no and the write is dropped; otherwise the guard lets `d` through, and the
counter adds it.

What must never happen: the counter's value ends lower than where it started.

This file models the guard as a function from `d` to "accepted `d`" or
"refused", applies accepted values by addition, and proves the value cannot
drop under any run of writes, accepted or refused, in any order, on create or
on update (the guard does not look at which one it is).
-/

namespace DefraConvergence.PCounterGuard

/-- The guard. A write of `d` is accepted (`some d`) only when `d` is not
    negative; a negative `d` is refused (`none`). Mirrors
    `CounterField::op` for `CType::PCounter`. -/
def op (d : Int) : Option Int :=
  -- Non-negative: let the value through unchanged.
  if 0 ≤ d then some d
  -- Negative: refuse ("value cannot be negative").
  else none

/-- What one write does to the counter's value `v`: add an accepted `d`, and
    leave `v` alone when the write was refused. -/
def apply (v : Int) : Option Int → Int
  -- Accepted: the value moves by `d`.
  | some d => v + d
  -- Refused: nothing is stored, the value stays.
  | none => v

/-- Run a whole sequence of writes, left to right, through the guard. -/
def run (v : Int) (ds : List Int) : Int :=
  -- For each incoming `d`: guard it, then apply what the guard let through.
  ds.foldl (fun acc d => apply acc (op d)) v

/-- Promise: whatever the guard accepts is not negative. -/
theorem op_accepts_only_nonneg (d x : Int) (h : op d = some x) : 0 ≤ x := by
  -- Open the guard's definition.
  unfold op at h
  -- Split on the guard's test.
  split at h
  -- Accepted branch: `x` is `d`, and the test said `0 ≤ d`.
  · cases h; assumption
  -- Refused branch: `none = some x` is impossible.
  · cases h

/-- Promise: one write never lowers the value. -/
theorem apply_op_ge (v d : Int) : v ≤ apply v (op d) := by
  -- Open the guard so both outcomes are visible.
  unfold op
  -- Case on the guard's test.
  split
  -- Accepted: the value becomes `v + d` with `0 ≤ d`.
  · simp only [apply]; omega
  -- Refused: the value is unchanged.
  · simp only [apply]; omega

/-- Promise: under any sequence of writes the final value is at least the
    starting value, so a positive counter never decreases. -/
theorem run_ge (ds : List Int) : ∀ v : Int, v ≤ run v ds := by
  -- Induct on the writes still to come.
  induction ds with
  -- No writes: the value is where it started.
  | nil => intro v; simp [run]
  -- One more write `d`, then the rest `ds`.
  | cons d ds ih =>
    intro v
    -- The first write moves `v` to `apply v (op d)`; the rest cannot go below that.
    have h1 := apply_op_ge v d
    have h2 := ih (apply v (op d))
    -- Unfold the run by one step and combine the two facts.
    simp only [run, List.foldl_cons] at h2 ⊢
    omega

end DefraConvergence.PCounterGuard
