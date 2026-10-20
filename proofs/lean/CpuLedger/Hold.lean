/-!
# CpuLedger.Hold: a claim alone takes all of its CPUs or none of them

The story, in plain words. A process keeps one counter per CPU: how many live
worker pools hold that CPU. A pool that wants its CPUs to itself claims them
alone: it walks its list of CPUs and, for each one, moves the counter from 0
to 1 in one compare-and-swap. If it reaches a CPU whose counter is not 0 (or a
CPU id the ledger has no counter for), it gives back every CPU it took so far,
one step down each, and fails.

A tiny example: counters `[0, 1]` (CPU 1 is held). A claim of CPUs 0 and 1
raises CPU 0 to 1, finds CPU 1 at 1, lowers CPU 0 back to 0, and fails: the
counters are `[0, 1]` again, exactly as before.

What must never happen:
* a claim that fails leaves any counter different from before
  (`hold_all_or_nothing`, `rollback_restores`);
* a claim that wins took a CPU whose counter was not 0, or one outside the
  ledger (`hold_won_free`, `hold_outside_fails`).

These laws hold for any number of CPUs and any list a caller passes, in any
order and with repeats. The races between pools are the TLA+ model
`proofs/tla/CpuLedger.tla`; this file carries the law that model takes on
trust about one claim, modeled as its sequential meaning.

Rust anchors: `Ledger::hold` and `Hold::drop` (`crates/workers/src/ledger.rs`).
-/

namespace CpuLedger

-- The counters, one per CPU id: how many live pools hold that CPU (`PROCESS` in ledger.rs).
abbrev Counts := Nat → Nat

-- One CPU's counter raised by one: the winning compare_exchange(0, 1), or share's fetch_add(1).
def bump (counts : Counts) (cpu : Nat) : Counts :=
  -- CPU `cpu` reads one more; every other CPU reads as before.
  fun c => if c = cpu then counts c + 1 else counts c

-- One CPU's counter lowered by one: Hold::drop's fetch_sub(1).
def drop1 (counts : Counts) (cpu : Nat) : Counts :=
  -- CPU `cpu` reads one less; every other CPU reads as before.
  fun c => if c = cpu then counts c - 1 else counts c

-- Each CPU of a list raised once; the head of the list is the one raised last.
def bumpAll (counts : Counts) : List Nat → Counts
  -- Nothing to raise: the counters are as they were.
  | [] => counts
  -- Raise the rest first, then the head.
  | cpu :: rest => bump (bumpAll counts rest) cpu

-- A Hold dropped: each CPU it lists lowered once, the head first.
def giveBack (counts : Counts) : List Nat → Counts
  -- Nothing listed: nothing to lower.
  | [] => counts
  -- Lower the head, then give back the rest.
  | cpu :: rest => giveBack (drop1 counts cpu) rest

-- Ledger::hold's loop over the CPUs still to go, with `taken` the CPUs won so far (newest first)
-- and `size` the number of counters the ledger has. It returns the counters after it, and the
-- CPUs held (`some`, the Hold) or `none` when the claim failed.
def holdLoop (size : Nat) (counts : Counts) (taken : List Nat) : List Nat → Counts × Option (List Nat)
  -- Every CPU was won: the Hold lists all of them.
  | [] => (counts, some taken)
  -- The next CPU:
  | cpu :: rest =>
    -- it has a counter and the counter reads 0, so the swap wins:
    if cpu < size ∧ counts cpu = 0 then
      -- raise it, record it as taken, and go on with the rest;
      holdLoop size (bump counts cpu) (cpu :: taken) rest
    -- otherwise the swap loses:
    else
      -- give back everything taken so far, and fail.
      (giveBack counts taken, none)

-- Ledger::hold: the loop, starting with nothing taken.
def hold (size : Nat) (counts : Counts) (cpus : List Nat) : Counts × Option (List Nat) :=
  -- Run the loop from an empty record.
  holdLoop size counts [] cpus

-- Promise: lowering a CPU just raised puts every counter back as it was.
theorem drop1_bump (counts : Counts) (cpu : Nat) :
    -- Raise `cpu`, then lower it: the counters are the ones we started with.
    drop1 (bump counts cpu) cpu = counts := by
  -- Two counter tables are equal when they agree on every CPU `c`.
  funext c
  -- Open both definitions, so the goal is about plain if-then-else.
  simp only [drop1, bump]
  -- Split on whether `c` is the CPU that was raised and lowered.
  by_cases e : c = cpu
  -- Same CPU: both ifs take their first branch.
  · rw [if_pos e, if_pos e]
    -- One up then one down is no change.
    omega
  -- Another CPU: both ifs take their second branch, and the goal closes as it is.
  · rw [if_neg e, if_neg e]

-- Promise: rollback after a partial claim restores the exact prior counters.
theorem rollback_restores (base : Counts) (taken : List Nat) :
    -- Raise the CPUs taken, then give all of them back: the counters are `base` again.
    giveBack (bumpAll base taken) taken = base := by
  -- Go through the record one CPU at a time.
  induction taken with
  -- Nothing taken: nothing raised, nothing given back.
  | nil => simp [bumpAll, giveBack]
  -- One more CPU on top of a record `rest` we already know about:
  | cons cpu rest ih =>
    -- the last CPU raised is the first given back, and lowering it undoes raising it,
    simp only [bumpAll, giveBack, drop1_bump]
    -- which leaves exactly the shorter case.
    exact ih

-- Promise: the loop, started on counters that carry the record `taken` on top of `base`,
-- either wins with every CPU raised once more, or fails with the counters back at `base`.
theorem holdLoop_all_or_nothing (size : Nat) (base : Counts) (cpus : List Nat) :
    -- For any record `taken` the loop starts with,
    ∀ taken : List Nat,
      -- either it wins, holding the CPUs it went through and the record, all raised,
      holdLoop size (bumpAll base taken) taken cpus
          = (bumpAll base (cpus.reverse ++ taken), some (cpus.reverse ++ taken))
        -- or it fails and leaves the counters exactly at `base`.
        ∨ holdLoop size (bumpAll base taken) taken cpus = (base, none) := by
  -- Go through the CPUs still to go, one at a time.
  induction cpus with
  -- No CPU left:
  | nil =>
    -- whatever the record,
    intro taken
    -- the loop wins,
    left
    -- holding exactly the record, and the counters carry it.
    simp [holdLoop]
  -- A next CPU `cpu`, then `rest`:
  | cons cpu rest ih =>
    -- whatever the record,
    intro taken
    -- either the swap on `cpu` wins or it loses.
    by_cases h : cpu < size ∧ bumpAll base taken cpu = 0
    -- It wins:
    · -- one turn of the loop raises `cpu`, records it, and goes on with `rest`;
      have hstep : holdLoop size (bumpAll base taken) taken (cpu :: rest)
          = holdLoop size (bump (bumpAll base taken) cpu) (cpu :: taken) rest := by
        -- Open the loop on `cpu :: rest` and take the winning branch; both sides then match.
        simp only [holdLoop, if_pos h]
      -- the rest of the loop starts with `cpu` added to the record, which we know about;
      have hrest := ih (cpu :: taken)
      -- and the CPUs gone through, newest first, are the same list either way.
      have horder : (cpu :: rest).reverse ++ taken = rest.reverse ++ cpu :: taken := by
        -- Reversing puts `cpu` last, right before the old record.
        simp
      -- Replace the loop by its next turn, and write the list the way the shorter case does.
      rw [hstep, horder]
      -- Raising `cpu` on top of the record is the record with `cpu` added, so this is the shorter case.
      exact hrest
    -- It loses:
    · -- one turn of the loop gives back the record, which puts the counters back at `base`;
      have hstep : holdLoop size (bumpAll base taken) taken (cpu :: rest) = (base, none) := by
        -- Open the loop, take the losing branch, and undo the record.
        simp only [holdLoop, if_neg h, rollback_restores]
      -- so the loop fails,
      right
      -- exactly as that turn says.
      exact hstep

-- Promise: a claim alone over any list of CPUs either takes all of them or leaves the
-- counters exactly as they were.
theorem hold_all_or_nothing (size : Nat) (counts : Counts) (cpus : List Nat) :
    -- Either it wins: every CPU raised once and the Hold lists them (newest first),
    hold size counts cpus = (bumpAll counts cpus.reverse, some cpus.reverse)
      -- or it fails, holding nothing and changing nothing.
      ∨ hold size counts cpus = (counts, none) := by
  -- The loop law, started with nothing taken.
  have hloop := holdLoop_all_or_nothing size counts cpus []
  -- With an empty record, "the CPUs then the record" is just the CPUs, and nothing is raised yet.
  simp only [List.append_nil, bumpAll] at hloop
  -- That is the promise, since `hold` is the loop started with nothing taken.
  exact hloop

-- Promise: a loop that wins found every CPU it went through inside the ledger and at zero.
theorem holdLoop_won_free (size : Nat) (cpus : List Nat) :
    -- For any counters, record, and result,
    ∀ (counts : Counts) (taken won : List Nat) (after : Counts),
      -- if the loop won,
      holdLoop size counts taken cpus = (after, some won) →
      -- then each CPU it went through had a counter, and that counter read 0.
      ∀ c ∈ cpus, c < size ∧ counts c = 0 := by
  -- Go through the CPUs one at a time.
  induction cpus with
  -- No CPU: there is nothing to check.
  | nil =>
    -- Name the counters, record, result, the win, and a CPU said to be in the empty list;
    intro counts taken won after _ c hc
    -- no CPU is in the empty list, so this case cannot happen.
    simp at hc
  -- A next CPU `cpu`, then `rest`:
  | cons cpu rest ih =>
    -- name the counters, the record, the result, the win, and the CPU `c` to check;
    intro counts taken won after hrun c hc
    -- the swap on `cpu` either wins or loses.
    by_cases h : cpu < size ∧ counts cpu = 0
    -- It wins:
    · -- one turn of the loop raises `cpu`, records it, and goes on with `rest`;
      have hstep : holdLoop size counts taken (cpu :: rest)
          = holdLoop size (bump counts cpu) (cpu :: taken) rest := by
        -- Open the loop on `cpu :: rest` and take the winning branch; both sides then match.
        simp only [holdLoop, if_pos h]
      -- so the rest of the loop is what won;
      rw [hstep] at hrun
      -- so every CPU of `rest` was inside the ledger and at zero after raising `cpu`.
      have hrest := ih (bump counts cpu) (cpu :: taken) won after hrun
      -- `c` is either `cpu` itself or one of `rest`.
      rw [List.mem_cons] at hc
      -- Look at both.
      cases hc with
      -- `c` is `cpu`:
      | inl e =>
        -- turn the goal into the claim about `cpu`,
        rw [e]
        -- which is exactly why the swap won.
        exact h
      -- `c` is one of `rest`:
      | inr hmem =>
        -- it was inside the ledger and at zero once `cpu` was raised.
        have hc' := hrest c hmem
        -- Either `c` is `cpu` again (a repeat in the list) or another CPU.
        by_cases e : c = cpu
        -- A repeat:
        · -- its counter after raising `cpu` read 0,
          have hbump := hc'.2
          -- open the raise,
          simp only [bump] at hbump
          -- which added one to `c`, since `c` is `cpu`;
          rw [if_pos e] at hbump
          -- a counter plus one is never 0, so this case cannot happen,
          exfalso
          -- as plain arithmetic shows.
          omega
        -- Another CPU:
        · -- its counter after raising `cpu` read 0,
          have hbump := hc'.2
          -- open the raise,
          simp only [bump] at hbump
          -- which left `c` alone;
          rw [if_neg e] at hbump
          -- so `c` was inside the ledger and at zero before, too.
          exact ⟨hc'.1, hbump⟩
    -- It loses:
    · -- one turn of the loop gives back the record and returns `none`;
      have hstep : holdLoop size counts taken (cpu :: rest) = (giveBack counts taken, none) := by
        -- Open the loop on `cpu :: rest` and take the losing branch; both sides then match.
        simp only [holdLoop, if_neg h]
      -- so the result we were told is a win is that `none`,
      rw [hstep] at hrun
      -- which is not a win, so this case cannot happen.
      cases hrun

-- Promise: a claim alone that wins found every CPU it asked for inside the ledger and free.
theorem hold_won_free (size : Nat) (counts : Counts) (cpus : List Nat)
    -- Given the counters after the claim and the CPUs its Hold lists,
    (after : Counts) (won : List Nat)
    -- and given that the claim won,
    (hwon : hold size counts cpus = (after, some won)) :
    -- every CPU it asked for had a counter, and that counter read 0 before.
    ∀ c ∈ cpus, c < size ∧ counts c = 0 :=
  -- The loop law, with nothing taken at the start.
  holdLoop_won_free size cpus counts [] won after hwon

-- Promise: a claim alone that wins holds exactly the CPUs it asked for, each raised once.
theorem hold_won_list (size : Nat) (counts : Counts) (cpus : List Nat)
    -- Given the counters after the claim and the CPUs its Hold lists,
    (after : Counts) (won : List Nat)
    -- and given that the claim won,
    (hwon : hold size counts cpus = (after, some won)) :
    -- the Hold lists the CPUs asked for, and each of them was raised once.
    won = cpus.reverse ∧ after = bumpAll counts cpus.reverse := by
  -- The claim took all or nothing.
  cases hold_all_or_nothing size counts cpus with
  -- It took all:
  | inl hall =>
    -- so the win we were given is that result,
    rw [hall] at hwon
    -- which fixes both the counters and the list;
    cases hwon
    -- and then both sides are the same.
    exact ⟨rfl, rfl⟩
  -- It took nothing:
  | inr hnone =>
    -- so the claim returned `none`,
    rw [hnone] at hwon
    -- which is not a win, so this case cannot happen.
    cases hwon

-- Promise: a claim alone that asks for a CPU the ledger has no counter for fails and changes
-- nothing (`holds.get(cpu)` is `None` in ledger.rs).
theorem hold_outside_fails (size : Nat) (counts : Counts) (cpus : List Nat) (c : Nat)
    -- Given a CPU `c` in the claim
    (hc : c ∈ cpus)
    -- whose id is past the last counter,
    (hout : size ≤ c) :
    -- the claim fails and the counters are as before.
    hold size counts cpus = (counts, none) := by
  -- The claim took all or nothing.
  cases hold_all_or_nothing size counts cpus with
  -- Say it took all:
  | inl hall =>
    -- then every CPU of it had a counter, `c` included,
    have hfree := hold_won_free size counts cpus _ _ hall c hc
    -- that is, `c` is below the number of counters,
    have hlt := hfree.1
    -- which contradicts `c` being past the last one,
    exfalso
    -- as plain arithmetic shows.
    omega
  -- It took nothing: that is the promise.
  | inr hnone =>
    -- Use it as it is.
    exact hnone

end CpuLedger
