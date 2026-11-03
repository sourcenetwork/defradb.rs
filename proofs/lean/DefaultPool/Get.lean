import DefaultPool.Slot
-- The registration slot and its laws: `select`, `select_runs`.

/-!
# DefaultPool.Get: what `default_pool()` answers

The story, in plain words. `default_pool()` first looks at the registration
slot: a registered pool that runs is the answer. Only when none runs does it
answer the started pool. `Pool::stop` uses this to time its own deadline: it
shuts its pool down first and then asks, so the pool it stops no longer runs
and cannot be the answer.

A tiny example. Pool 4 is registered and runs, and pool 9 was started on
first use. `default_pool()` answers 4. Pool 4 is stopped: it shuts down, so
it no longer runs, and the stop's own call answers 9.

What must never happen:
* a running registered pool is passed over for the started one
  (`registered_wins`);
* the started pool is answered while a registered pool runs
  (`started_only_when_none_runs`);
* a stop times its deadline on the very pool it is stopping
  (`stop_deadline_elsewhere`).

The moments in between (the slot loaded, then the pool checked, a pool
registering or closing between them) are the TLA+ model
`proofs/tla/DefaultPool.tla`; this file proves the choice itself for any
pools.

Rust anchors: `DefaultPool::get` (`crates/workers/src/pool/default.rs`) and
`Pool::stop` (`crates/workers/src/pool/mod.rs`, its shutdown before its
deadline's `timeout`).
-/

namespace DefaultPool

-- What `default_pool()` answers: the registered pool if it runs, else the started pool.
def defaultOf (running : Running) (slot : Slot) (started : Nat) : Nat :=
  -- Look at the registered side:
  match select running slot with
  -- a registered pool runs: it is the answer;
  | some p => p
  -- none runs: the started pool is the answer.
  | none => started

-- Promise: a running registered pool wins over the started one.
theorem registered_wins (running : Running) (slot : Slot) (started p : Nat)
    -- Given that the registered side answers `p`,
    (h : select running slot = some p) :
    -- `default_pool()` answers `p`.
    defaultOf running slot started = p := by
  -- Unfold the answer; its look at the slot finds `p`.
  simp [defaultOf, h]

-- Promise: the started pool is the answer when no registered pool runs.
theorem started_only_when_none_runs (running : Running) (slot : Slot) (started : Nat)
    -- Given that no registered pool runs,
    (h : select running slot = none) :
    -- `default_pool()` answers the started pool.
    defaultOf running slot started = started := by
  -- Unfold the answer; its look at the slot finds nothing running.
  simp [defaultOf, h]

-- Promise: a stop never times its deadline on the pool it stops, because it shut
-- that pool down before asking (and the started pool is not the one it stops).
theorem stop_deadline_elsewhere (running : Running) (slot : Slot) (started p : Nat)
    -- Given that `p`, the pool being stopped, no longer runs,
    (hp : running p = false)
    -- and that `p` is not the started pool,
    (hs : started ≠ p) :
    -- `default_pool()` does not answer `p`.
    defaultOf running slot started ≠ p := by
  -- Suppose it did answer `p`.
  intro h
  -- Split on the registered side.
  cases hsel : select running slot with
  -- Nothing registered runs:
  | none =>
    -- the answer was the started pool, so `h` says the started pool is `p`;
    simp [defaultOf, hsel] at h
    -- which `hs` forbids.
    exact hs h
  -- A registered pool `q` runs:
  | some q =>
    -- the answer was `q`, so `h` says `q` is `p`;
    simp [defaultOf, hsel] at h
    -- `q` runs, being the registered answer;
    have hq := select_runs running slot q hsel
    -- so `p` runs;
    rw [h] at hq
    -- which `hp` contradicts: rewriting gives `false = true`.
    simp [hp] at hq

end DefaultPool
