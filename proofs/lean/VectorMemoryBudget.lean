/-!
# VectorMemoryBudget: the node's vector memory budget never loses or invents bytes

The story, in plain words. A node keeps some of its vector indexes' data in
memory: DiskANN codes, codes a search read on a miss, copies of pending lists.
The operator gives the node a budget of bytes for all of it. The budget is one
counter of bytes held, shared by every worker. A cache takes room before it
keeps something (a charge) and gives it back when it lets that go.

What must never happen:
* a take lets the counter pass the limit (`take_within_limit`);
* the counter stops adding up to what the charges hold (`take_conserves`,
  `give_conserves`, `give_all_restores`);
* a refused take changes anything (`take_refused`);
* a reload's `cover` or `settle` leaves its charge short of what it asked for
  while claiming success (`cover_reaches`, `settle_exact`);
* replacing a current charge with a larger one changes the counter by exactly
  the difference and never passes the limit (`replace_within_limit`,
  `replace_conserves`).

The last section shows why the take is one step: checking for room and adding
in two steps lets two takers through the same room (`split_check_overshoots`).

The temporal story (reloads, claims, misses, merges) is the TLA+ model
`proofs/tla/VectorMemoryBudget.tla`; this file carries the arithmetic it takes
on trust by giving every code one unit.

Rust anchors: `MemoryBudget::take` (a compare-and-swap that adds only when the
sum fits), `MemoryBudget::give`, and `Charge::grow`, `Charge::cover`,
`Charge::settle`, `Charge::release` in
`crates/database/db/src/index/vector/budget.rs`.
-/

namespace VectorMemoryBudget

-- The bytes a list of charges holds, added up.
def held (charges : List Nat) : Nat := charges.foldr (· + ·) 0

-- Take `n` bytes from a budget with `limit` bytes, `used` of them held: the
-- new count if they fit, nothing otherwise. One step, as the Rust CAS is.
def take (limit used n : Nat) : Option Nat :=
  -- They fit beside what is held: the counter grows by exactly `n`;
  if used + n ≤ limit then some (used + n)
  -- they do not: nothing is taken.
  else none

-- Give `n` bytes back.
def give (used n : Nat) : Nat := used - n

-- Replace `released` bytes held by a current copy with `bytes` held by its
-- replacement: the new count if they fit, nothing otherwise. One step, as the
-- Rust compare-and-swap is.
def replace (limit used released bytes : Nat) : Option Nat :=
  -- The room the old copy held goes back, and the new copy takes its room.
  if released ≤ used ∧ used - released + bytes ≤ limit then
    some (used - released + bytes)
  -- Either the old charge is not held, or the new one does not fit.
  else none

-- Promise: after a successful take the counter is still within the limit.
theorem take_within_limit (limit used n used' : Nat)
    -- Given the take of `n` bytes succeeded with new count `used'`,
    (h : take limit used n = some used') :
    -- the new count is at most the limit.
    used' ≤ limit := by
  -- Unfold the take and split on whether the bytes fit.
  unfold take at h
  split at h
  -- They fit: the new count is `used + n`, which the condition bounds.
  · cases h; assumption
  -- They did not fit: the take returned nothing, contradicting success.
  · cases h

-- Promise: a successful take keeps the counter equal to what the charges hold.
theorem take_conserves (limit used n used' : Nat) (charges : List Nat)
    -- Given the counter adds up before the take,
    (hinv : used = held charges)
    -- and the take succeeded,
    (h : take limit used n = some used') :
    -- the new charge joins the list and the counter still adds up.
    used' = held (n :: charges) := by
  -- Unfold the take and split on whether the bytes fit.
  unfold take at h
  split at h
  -- They fit: both sides are `used + n`.
  · cases h; simp only [held, List.foldr] at *; omega
  -- They did not fit: contradiction with success.
  · cases h

-- Promise: a refused take means the bytes did not fit, and nothing changed.
theorem take_refused (limit used n : Nat)
    -- Given the take was refused,
    (h : take limit used n = none) :
    -- the bytes would have passed the limit.
    limit < used + n := by
  -- Unfold the take and split on whether the bytes fit.
  unfold take at h
  split at h
  -- They fit: the take would have succeeded, contradicting the refusal.
  · cases h
  -- They did not fit: that is the promise, by arithmetic.
  · omega

-- Promise: giving a charge back keeps the counter adding up.
theorem give_conserves (used n : Nat) (charges : List Nat)
    -- Given the counter adds up while the charge of `n` bytes is held,
    (hinv : used = held (n :: charges)) :
    -- after giving it back the counter is what the rest hold.
    give used n = held charges := by
  -- Unfold both sides; it is plain subtraction.
  simp only [give, held, List.foldr] at *
  omega

-- Promise: once every charge is given back, the counter is zero again.
theorem give_all_restores (used : Nat) (charges : List Nat)
    -- Given the counter adds up with these charges held,
    (hinv : used = held charges) :
    -- giving them all back, one at a time, leaves nothing held.
    charges.foldl give used = 0 := by
  -- Induct on the charges, carrying the counter along.
  induction charges generalizing used with
  -- No charges: the counter is already zero.
  | nil => simpa [held] using hinv
  -- One charge then the rest: give the first back and use the rest's promise.
  | cons n rest ih =>
    -- The rest's promise needs the counter to add up after the first give.
    apply ih
    -- That is give_conserves for the first charge.
    exact give_conserves used n rest hinv

-- Promise: a replacement never passes the limit.
theorem replace_within_limit (limit used released bytes used' : Nat)
    -- Given the replacement succeeded with new count `used'`,
    (h : replace limit used released bytes = some used') :
    -- the new count is at most the limit.
    used' ≤ limit := by
  -- Unfold and split on the replacement condition.
  unfold replace at h
  split at h
  -- The fitted branch returns the bounded count.
  · cases h; omega
  -- The refused branch contradicts success.
  · cases h

-- Promise: replacing a charge conserves the counter.
theorem replace_conserves (limit used released bytes used' : Nat)
    (heldOld : released ≤ used) (heldNew : used - released + bytes ≤ limit)
    (h : replace limit used released bytes = some used') :
    used' = used - released + bytes := by
  -- Unfold and take the fitted branch; the side conditions exclude refusal.
  unfold replace at h
  rw [if_pos ⟨heldOld, heldNew⟩] at h
  cases h
  rfl

-- A reload's cover: hold at least `target` bytes, taking a whole `step` where
-- it fits and only what is missing otherwise. Returns the charge's new size
-- and the counter, or nothing when even what is missing does not fit.
def cover (limit used charge target step : Nat) : Option (Nat × Nat) :=
  -- Already enough: nothing changes.
  if target ≤ charge then some (charge, used)
  else
    -- What is missing, and the bigger amount a step would take.
    let missing := target - charge
    let big := max missing step
    -- Try the whole step first;
    match take limit used big with
    | some used' => some (charge + big, used')
    -- then only what is missing;
    | none =>
      match take limit used missing with
      | some used' => some (charge + missing, used')
      -- and if that does not fit either, the cover fails.
      | none => none

-- Promise: a successful cover holds at least its target, within the limit,
-- and the counter grew by exactly what the charge grew.
theorem cover_reaches (limit used charge target step charge' used' : Nat)
    -- Given the counter was within the limit before,
    (hle : used ≤ limit)
    -- and the cover succeeded,
    (h : cover limit used charge target step = some (charge', used')) :
    -- the charge reaches the target, the counter stays within the limit,
    -- and both grew by the same amount.
    target ≤ charge' ∧ used' ≤ limit ∧ used' + charge = used + charge' := by
  -- Unfold the cover and follow each of its branches.
  unfold cover at h
  split at h
  -- Already enough: nothing changed.
  · cases h; omega
  · -- Name the two amounts the cover can take, and drop the `let`s.
    rename_i hshort
    dsimp only at h
    -- Look at the whole-step take.
    cases hstep : take limit used (max (target - charge) step) with
    | some u =>
      -- It succeeded: the step covers what is missing, within the limit.
      simp only [hstep, Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl⟩ := h
      have := take_within_limit limit used _ u hstep
      unfold take at hstep; split at hstep <;> cases hstep
      refine ⟨by omega, this, by omega⟩
    | none =>
      -- It was refused: look at the exact take.
      simp only [hstep] at h
      cases hexact : take limit used (target - charge) with
      | some u =>
        -- It succeeded: exactly what is missing was taken.
        simp only [hexact, Option.some.injEq, Prod.mk.injEq] at h
        obtain ⟨rfl, rfl⟩ := h
        have := take_within_limit limit used _ u hexact
        unfold take at hexact; split at hexact <;> cases hexact
        refine ⟨by omega, this, by omega⟩
      -- Neither fit: the cover returned nothing, contradicting success.
      | none => simp [hexact] at h

-- A settle: hold exactly `target` bytes, giving back the excess or taking
-- what is missing.
def settle (limit used charge target : Nat) : Option (Nat × Nat) :=
  -- Holding too much: give the excess back.
  if target < charge then some (target, used - (charge - target))
  -- Holding too little or exactly enough: take what is missing.
  else (take limit used (target - charge)).map fun used' => (target, used')

-- Promise: a successful settle holds exactly its target, and the counter moved
-- by exactly the difference, staying within the limit.
theorem settle_exact (limit used charge target charge' used' : Nat)
    -- Given the counter was within the limit and covers this charge,
    (hle : used ≤ limit) (hheld : charge ≤ used)
    -- and the settle succeeded,
    (h : settle limit used charge target = some (charge', used')) :
    -- the charge is the target, and the counter changed with it.
    charge' = target ∧ used' ≤ limit ∧ used' + charge = used + charge' := by
  -- Unfold the settle and split on whether it gives back or takes.
  unfold settle at h
  split at h
  -- It gave the excess back.
  · cases h; omega
  -- It took what was missing.
  · rename_i hnot
    -- Name the take's result.
    cases htake : take limit used (target - charge) with
    -- A refused take makes the settle fail, contradicting success.
    | none => simp [htake] at h
    -- A take that succeeded: the counter grew by what was missing.
    | some u =>
      simp [htake] at h
      obtain ⟨rfl, rfl⟩ := h
      have := take_within_limit limit used _ u htake
      unfold take at htake; split at htake <;> cases htake
      refine ⟨rfl, this, by omega⟩

-- Checking for room in one step and adding in another: the check alone.
def check (limit used n : Nat) : Bool := used + n ≤ limit

-- Promise (why the take is one step): two takers that both check before
-- either adds can both pass and push the counter past the limit.
theorem split_check_overshoots :
    -- There is a budget and a counter where
    ∃ limit used n : Nat,
      -- both takers' checks pass on the same count,
      check limit used n = true ∧ check limit used n = true ∧
      -- and adding both takes the counter past the limit.
      limit < used + n + n := by
  -- A budget of 1 with nothing held, and two takers of 1 byte each.
  exact ⟨1, 0, 1, by decide, by decide, by decide⟩

end VectorMemoryBudget
