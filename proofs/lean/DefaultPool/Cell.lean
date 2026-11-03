/-!
# DefaultPool.Cell: every caller agrees on the one started pool

The story, in plain words. When no registered pool runs, `default_pool()`
answers the pool started on first use. That pool lives in a cell that starts
empty and is written once. A caller that finds the cell empty starts a pool
and publishes it: if the cell is still empty its pool goes in, and if another
caller published first, its own pool is dropped and it takes the pool already
there. Either way the caller gets back the pool the cell holds.

A tiny example. Callers start pools 7, 8 and 9 at the same moment and publish
in that order. Pool 7 goes into the empty cell; 8 and 9 find 7 there, are
dropped, and their callers get 7. All three callers get pool 7.

What must never happen:
* a publish replaces the pool already in the cell (`publish_keeps`);
* a caller that publishes after the cell was filled gets anything but the
  pool in it (`every_later_caller_gets_winner`);
* callers racing on an empty cell get two different pools: two process-wide
  defaults (`first_calls_agree`).

Who may start, and when (one start at a time through the cell's turn, a look
at the cell after the turn, the turn given back on a failure), is the TLA+
model `proofs/tla/DefaultPool.tla`; this file proves the publish law for any
number of callers.

Rust anchors: `DefaultPool::started` (`crates/workers/src/pool/default.rs`)
over regolith's `OnceCell::get_or_try_init` and its `publish`, one
`compare_exchange` from empty (`regolith 0.2.1 src/sync/once_cell.rs`),
modeled here as its sequential meaning.
-/

namespace DefaultPool

-- One publish of the pool `mine` into the cell: the cell after it, and the pool the caller gets back.
def publish (cell : Option Nat) (mine : Nat) : Option Nat × Nat :=
  -- Look at the cell:
  match cell with
  -- a pool is already there: the cell keeps it and the caller gets it (its own pool is dropped);
  | some winner => (some winner, winner)
  -- the cell is empty: the caller's pool goes in, and the caller gets it.
  | none => (some mine, mine)

-- Several publishes, one after another: the pool each caller gets back, in order.
def publishAll : Option Nat → List Nat → List Nat
  -- No more callers: no more answers.
  | _, [] => []
  -- The next caller publishes its pool; the rest publish into the cell it leaves.
  | cell, m :: ms => (publish cell m).2 :: publishAll (publish cell m).1 ms

-- Promise: once a pool is in the cell, a publish never replaces it.
theorem publish_keeps (w m : Nat) :
    -- Publishing `m` into a cell holding `w` leaves `w` there.
    (publish (some w) m).1 = some w := by
  -- Compute the publish: the cell is full, so it is unchanged.
  rfl

-- Promise: once the cell holds `w`, every later caller gets `w` back.
theorem every_later_caller_gets_winner (w : Nat) (ms : List Nat) :
    -- Every pool handed back to the callers `ms` is `w`.
    ∀ a, a ∈ publishAll (some w) ms → a = w := by
  -- Induction on the list of callers.
  induction ms with
  -- No callers:
  | nil =>
    -- take any pool `a` said to be handed back,
    intro a ha
    -- but nothing is handed back, so `ha` is impossible.
    simp [publishAll] at ha
  -- A caller `m`, then the callers `ms`:
  | cons m ms ih =>
    -- take any pool `a` handed back,
    intro a ha
    -- `m` gets `w` and leaves the cell holding `w`: so `a` is `w` or was handed back to `ms`.
    simp [publishAll, publish] at ha
    -- Look at which of the two it is.
    cases ha with
    -- `a` is `w`: done.
    | inl h => exact h
    -- `a` was handed back to `ms`, on a cell still holding `w`: the promise for `ms` says it is `w`.
    | inr h => exact ih a h

-- Promise: callers racing on an empty cell all get the first publisher's pool.
theorem first_calls_agree (m : Nat) (ms : List Nat) :
    -- Every pool handed back to `m` and the callers after it is `m`'s pool.
    ∀ a, a ∈ publishAll none (m :: ms) → a = m := by
  -- Take any pool `a` handed back.
  intro a ha
  -- `m` fills the empty cell and gets its own pool: so `a` is `m` or was handed back later.
  simp [publishAll, publish] at ha
  -- Look at which of the two it is.
  cases ha with
  -- `a` is `m`: done.
  | inl h => exact h
  -- `a` was handed back later, on a cell holding `m`: the promise above says it is `m`.
  | inr h => exact every_later_caller_gets_winner m ms a h

end DefaultPool
