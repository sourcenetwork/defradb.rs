/-!
# DefaultPool.Slot: the first registered pool that runs is the default

The story, in plain words. A process has one registration slot that names at
most one worker pool. A pool asks to be the default with `make_default`: the
call looks at the pool the slot names; if that pool still runs, the slot keeps
it and the call answers whether it is the caller's own pool; if it does not
run (it is closing, or gone), the caller's pool takes the slot and the answer
is "yes". `default_pool()` answers the pool the slot names, but only while it
runs.

A tiny example. Pools 1, 2 and 3 call `make_default` in that order, and only
pools 2 and 3 run. Pool 1 takes the empty slot, but it does not run; pool 2
replaces it, because pool 1 does not run; pool 3 finds pool 2 running and
leaves it. The default is pool 2: the first pool of the list that runs.

What must never happen:
* the registered answer is a pool that does not run (`select_runs`,
  `stopped_never_selected`);
* a call replaces a registered pool that still runs, so two running pools take
  turns being the default (`register_keeps_running`,
  `register_moves_only_off_stopped`);
* after any number of calls the default is anything but the first calling pool
  that runs (`first_running_wins`), or calls made after it change it
  (`later_registrations_change_nothing`);
* `make_default` answers "yes" when the slot does not name its pool, or "no"
  when no other running pool holds the slot (`answer_yes_names_pool`,
  `answer_no_means_other_runs`).

The races between calls (a load, a pool closing, the compare-and-swap that
fails and loads again) are the TLA+ model `proofs/tla/DefaultPool.tla`; this
file proves, for any number of pools and calls, the laws one call obeys and
what a whole sequence of calls leaves behind.

Rust anchors: `DefaultPool::register` and `running` in
`crates/workers/src/pool/default.rs` (a kovan `Atom` load and
`compare_and_swap`, an `Arc::ptr_eq`, a `Weak::upgrade` and an `is_closing`
load), modeled here as their sequential meaning. Which pools run is a fixed
function during the calls: a pool that stopped never runs again.
-/

namespace DefaultPool

-- Which pools run right now: a pool is a number, and `running p` says whether pool `p` runs.
abbrev Running := Nat → Bool

-- The registration slot: the pool it names, or nothing.
abbrev Slot := Option Nat

-- What the registered side of `default_pool()` answers: the pool the slot names, while it runs.
def select (running : Running) : Slot → Option Nat
  -- Nothing is registered: there is no registered answer.
  | none => none
  -- A pool is registered: it is the answer if it runs, and there is none otherwise.
  | some p => if running p then some p else none

-- One `make_default` call by `pool` on `slot`: the slot after the call.
def register (running : Running) (slot : Slot) (pool : Nat) : Slot :=
  -- Look at the registered pool, if it still runs:
  match select running slot with
  -- one runs: the slot keeps it;
  | some current => some current
  -- none runs: the caller's pool takes the slot.
  | none => some pool

-- What one `make_default` call by `pool` on `slot` returns.
def answer (running : Running) (slot : Slot) (pool : Nat) : Bool :=
  -- Look at the registered pool, if it still runs:
  match select running slot with
  -- one runs: "yes" exactly when it is the caller's own pool;
  | some current => decide (current = pool)
  -- none runs: the caller's pool took the slot, so "yes".
  | none => true

-- Several `make_default` calls, one after another, starting from `slot`.
def registerAll (running : Running) (slot : Slot) : List Nat → Slot
  -- No more calls: the slot stays as it is.
  | [] => slot
  -- The next call is `p`'s; the remaining calls work on the slot it leaves.
  | p :: ps => registerAll running (register running slot p) ps

-- The first pool of a list that runs, if there is one.
def firstRunning (running : Running) : List Nat → Option Nat
  -- An empty list has no running pool.
  | [] => none
  -- The head, if it runs; otherwise the first running pool of the rest.
  | p :: ps => if running p then some p else firstRunning running ps

-- A registered answer if there is one, else `rest`.
def registeredOr : Option Nat → Option Nat → Option Nat
  -- There is a registered answer: it wins.
  | some p, _ => some p
  -- There is none: take `rest`.
  | none, rest => rest

-- Promise: whatever the registered side answers is a pool that runs.
theorem select_runs (running : Running) (slot : Slot) (p : Nat)
    -- Given that the registered side answered `p`,
    (h : select running slot = some p) :
    -- `p` runs.
    running p = true := by
  -- Split on what the slot names.
  cases slot with
  -- Nothing: the answer would be nothing, which contradicts `h`.
  | none => simp [select] at h
  -- Some pool `q`:
  | some q =>
    -- split on whether `q` runs.
    cases hq : running q with
    -- It does not: the answer would be nothing, which contradicts `h`.
    | false => simp [select, hq] at h
    -- It does: then `h` says that `q` is `p`.
    | true =>
      -- Turn `h` into `q = p`.
      simp [select, hq] at h
      -- Put `q` for `p` (or `p` for `q`) everywhere.
      subst h
      -- The goal is now exactly the fact that `q` runs.
      exact hq

-- Promise: the registered answer is the pool the slot names.
theorem select_names_slot (running : Running) (slot : Slot) (c : Nat)
    -- Given that the registered side answered `c`,
    (h : select running slot = some c) :
    -- the slot names `c`.
    slot = some c := by
  -- Split on what the slot names.
  cases slot with
  -- Nothing: the answer would be nothing, which contradicts `h`.
  | none => simp [select] at h
  -- Some pool `q`:
  | some q =>
    -- split on whether `q` runs.
    cases hq : running q with
    -- It does not: the answer would be nothing, which contradicts `h`.
    | false => simp [select, hq] at h
    -- It does: then `h` says that `q` is `c`.
    | true =>
      -- Turn `h` into `q = c`.
      simp [select, hq] at h
      -- Rewrite `q` to `c`; both sides are then `some c`.
      rw [h]

-- Promise: a pool that does not run (closing, or gone) is never the registered answer.
theorem stopped_never_selected (running : Running) (slot : Slot) (p : Nat)
    -- Given that `p` does not run,
    (hp : running p = false) :
    -- the registered side never answers `p`.
    select running slot ≠ some p := by
  -- Suppose it did answer `p`.
  intro h
  -- Then `p` would run, by the promise above,
  have hr := select_runs running slot p h
  -- which `hp` contradicts: rewriting turns `hr` into `false = true`.
  simp [hp] at hr

-- Promise: a call never replaces a registered pool that still runs.
theorem register_keeps_running (running : Running) (slot : Slot) (pool c : Nat)
    -- Given that the slot names `c` and `c` runs,
    (h : select running slot = some c) :
    -- the slot still names `c` after the call.
    register running slot pool = some c := by
  -- Unfold the call; its look at the slot finds `c`.
  simp [register, h]

-- Promise: a call made while no registered pool runs puts its own pool in the slot.
theorem register_takes_empty (running : Running) (slot : Slot) (pool : Nat)
    -- Given that no registered pool runs,
    (h : select running slot = none) :
    -- the slot names the caller's pool after the call.
    register running slot pool = some pool := by
  -- Unfold the call; its look at the slot finds no running pool.
  simp [register, h]

-- Promise: a call moves the slot only off a pool that does not run.
theorem register_moves_only_off_stopped (running : Running) (slot : Slot) (pool : Nat)
    -- Given that the call changed the slot,
    (h : register running slot pool ≠ slot) :
    -- no registered pool ran before it.
    select running slot = none := by
  -- Split on the registered answer before the call.
  cases hs : select running slot with
  -- None ran: that is the goal.
  | none => rfl
  -- `c` ran:
  | some c =>
    -- this case cannot happen, so prove it is impossible:
    exfalso
    -- `h` says the slot changed; show instead that it did not.
    apply h
    -- The call keeps `c`, and the slot named `c` already: both sides are `some c`.
    rw [register_keeps_running running slot pool c hs, select_names_slot running slot c hs]

-- Promise: "yes" means the slot names the caller's pool after the call.
theorem answer_yes_names_pool (running : Running) (slot : Slot) (pool : Nat)
    -- Given that the call answered "yes",
    (h : answer running slot pool = true) :
    -- the slot names the caller's pool after it.
    register running slot pool = some pool := by
  -- Split on the registered answer before the call.
  cases hs : select running slot with
  -- None ran: the caller's pool took the slot.
  | none => simp [register, hs]
  -- `c` ran:
  | some c =>
    -- "yes" then means `c` is the caller's pool: turn `h` into `c = pool`.
    simp [answer, hs] at h
    -- The slot keeps `c`, which is the caller's pool.
    simp [register, hs, h]

-- Promise: "no" means another pool runs and holds the slot, and keeps it.
theorem answer_no_means_other_runs (running : Running) (slot : Slot) (pool : Nat)
    -- Given that the call answered "no",
    (h : answer running slot pool = false) :
    -- some other pool `c` was the registered answer, and the slot still names it after the call.
    ∃ c, select running slot = some c ∧ c ≠ pool ∧ register running slot pool = some c := by
  -- Split on the registered answer before the call.
  cases hs : select running slot with
  -- None ran: the answer would have been "yes", which contradicts `h`.
  | none => simp [answer, hs] at h
  -- `c` ran:
  | some c =>
    -- "no" then means `c` is not the caller's pool: turn `h` into `¬ c = pool`.
    simp [answer, hs] at h
    -- `c` is the witness: it was the answer, it is another pool, and the call keeps it.
    exact ⟨c, rfl, h, register_keeps_running running slot pool c hs⟩

-- Promise: after any list of calls, the registered answer is the one before them
-- if it still runs, and otherwise the first calling pool that runs.
theorem select_registerAll (running : Running) (ps : List Nat) (slot : Slot) :
    -- The answer after the calls is the earlier answer, or else the first running caller.
    select running (registerAll running slot ps) =
      registeredOr (select running slot) (firstRunning running ps) := by
  -- Prove it for every starting slot, by induction on the list of calls.
  induction ps generalizing slot with
  -- No calls:
  | nil =>
    -- the slot is unchanged, and an empty list has no running pool;
    simp only [registerAll, firstRunning]
    -- split on the registered answer: both sides then compute to it.
    cases select running slot <;> rfl
  -- A call by `p`, then the calls `ps`:
  | cons p ps ih =>
    -- the calls `ps` work on the slot that `p`'s call leaves;
    simp only [registerAll]
    -- by the promise for `ps`, their answer is `p`'s slot's answer, or the first running pool of `ps`.
    rw [ih]
    -- Split on whether a registered pool ran before `p`'s call.
    cases hs : select running slot with
    -- None ran:
    | none =>
      -- `p` takes the slot;
      rw [register_takes_empty running slot p hs]
      -- split on whether `p` runs; both sides then compute to the same answer.
      cases hp : running p <;> simp [select, firstRunning, registeredOr, hp]
    -- `c` ran:
    | some c =>
      -- the slot keeps `c`;
      rw [register_keeps_running running slot p c hs]
      -- `c` runs (it was the registered answer),
      have hc : running c = true := select_runs running slot c hs
      -- so both sides answer `c`.
      simp [select, registeredOr, hc]

-- Promise: starting from an empty slot, after any list of `make_default` calls the
-- default is the first calling pool that runs.
theorem first_running_wins (running : Running) (ps : List Nat) :
    -- The registered answer is the first running pool of the list.
    select running (registerAll running none ps) = firstRunning running ps := by
  -- The general promise, from the empty slot,
  have h := select_registerAll running ps none
  -- where the empty slot has no registered answer, so the first running pool is left.
  simpa [select, registeredOr] using h

-- Promise: the first running pool of a list is still the first after more pools are added.
theorem firstRunning_append (running : Running) (ps qs : List Nat) (p : Nat)
    -- Given that `p` is the first running pool of `ps`,
    (h : firstRunning running ps = some p) :
    -- it is also the first running pool of `ps` followed by `qs`.
    firstRunning running (ps ++ qs) = some p := by
  -- Induction on `ps`.
  induction ps with
  -- `ps` empty: it has no running pool, which contradicts `h`.
  | nil => simp [firstRunning] at h
  -- `ps` is `a` then `rest`:
  | cons a rest ih =>
    -- split on whether `a` runs.
    cases ha : running a with
    -- It does not: the first running pool is that of `rest`, for both lists.
    | false =>
      -- Turn `h` into: `p` is the first running pool of `rest`.
      simp [firstRunning, ha] at h
      -- The goal reduces the same way, and the promise for `rest` closes it.
      simpa [firstRunning, ha] using ih h
    -- It does: `a` is the first running pool of both lists, and `h` says `a` is `p`.
    | true =>
      -- Turn `h` into `a = p`.
      simp [firstRunning, ha] at h
      -- Put `a` for `p` (or `p` for `a`) everywhere, so only one name is left.
      subst h
      -- Compute the goal with that pool running: the head runs, so it is the answer.
      simp [firstRunning, ha]

-- Promise: once a running pool has registered, pools that register after it change nothing.
theorem later_registrations_change_nothing (running : Running) (ps qs : List Nat) (p : Nat)
    -- Given that `p` is the first running pool among the calls `ps`,
    (h : firstRunning running ps = some p) :
    -- the default is still `p` after the further calls `qs`.
    select running (registerAll running none (ps ++ qs)) = some p := by
  -- The default after all the calls is the first running pool of the whole list,
  rw [first_running_wins]
  -- which is still `p`.
  exact firstRunning_append running ps qs p h

end DefaultPool
