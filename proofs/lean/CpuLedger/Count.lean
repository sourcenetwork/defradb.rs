-- The counters, a claim alone, and a Hold's give-back come from CpuLedger.Hold.
import CpuLedger.Hold

/-!
# CpuLedger.Count: every counter is exactly the number of Holds that list its CPU

The story, in plain words. Each live pool holds a Hold: the list of CPUs whose
counters it raised. A pool that claims alone raises each CPU once, all or
nothing (`CpuLedger.Hold`). A pool that shares raises every CPU it asks for
that has a counter, whatever the counter was, and never fails. Dropping a
Hold lowers each CPU it lists once. The books are right when every counter
equals the number of Holds listing its CPU: then a CPU reads free (zero) only
when no Hold lists it.

A tiny example: pool A holds CPU 0 alone, pool B shares CPUs 0 and 1. The
Holds are `[[0], [1, 0]]` and the counters read 2 for CPU 0 and 1 for CPU 1.
A goes: its Hold gives CPU 0 back, CPU 0 reads 1, and B's CPUs both still read
held.

What must never happen:
* a claim alone, a share, or a dropped Hold leaves a counter that differs
  from the number of Holds listing its CPU (`hold_keeps_exact`,
  `share_keeps_exact`, `release_keeps_exact`);
* a CPU some Hold lists reads free (`held_never_free`).

These laws hold for any number of CPUs and pools. The races between pools,
and what a caller must uphold (keep the Hold while its threads run, give it
back once, run only on what it holds), are the TLA+ model
`proofs/tla/CpuLedger.tla`.

Rust anchors: `Ledger::hold`, `Ledger::share` and `Hold::drop`
(`crates/workers/src/ledger.rs`).
-/

namespace CpuLedger

-- How many times CPU `c` appears in a list of CPUs.
def times (c : Nat) : List Nat → Nat
  -- An empty list never lists `c`.
  | [] => 0
  -- One for the head if it is `c`, plus however often the rest lists it.
  | x :: rest => (if x = c then 1 else 0) + times c rest

-- How many times the Holds of a list name CPU `c`, all together.
def occupancy (c : Nat) : List (List Nat) → Nat
  -- No Hold: nobody holds `c`.
  | [] => 0
  -- The first Hold's count, plus the others'.
  | h :: rest => times c h + occupancy c rest

-- The books are right: every counter is the number of times the Holds name its CPU.
def Exact (counts : Counts) (holds : List (List Nat)) : Prop :=
  -- For every CPU, its counter equals its occupancy.
  ∀ c, counts c = occupancy c holds

-- Ledger::share's loop: each CPU that has a counter is raised and recorded; one without is
-- skipped. It never fails. It returns the counters after it and the Hold.
def shareLoop (size : Nat) (counts : Counts) (taken : List Nat) : List Nat → Counts × List Nat
  -- Every CPU gone through: the Hold lists what was recorded.
  | [] => (counts, taken)
  -- The next CPU:
  | cpu :: rest =>
    -- it has a counter:
    if cpu < size then
      -- raise it whatever it was, record it, and go on;
      shareLoop size (bump counts cpu) (cpu :: taken) rest
    -- it has none:
    else
      -- skip it, and go on.
      shareLoop size counts taken rest

-- Ledger::share: the loop, starting with nothing recorded.
def share (size : Nat) (counts : Counts) (cpus : List Nat) : Counts × List Nat :=
  -- Run the loop from an empty record.
  shareLoop size counts [] cpus

-- Promise: raising each CPU of a list once adds, to each counter, how often the list names it.
theorem bumpAll_apply (counts : Counts) (c : Nat) (l : List Nat) :
    -- After the raises, CPU `c` reads its old value plus its count in the list.
    bumpAll counts l c = counts c + times c l := by
  -- Go through the list one CPU at a time.
  induction l with
  -- Empty list: nothing raised, and the count is zero.
  | nil => simp [bumpAll, times]
  -- A head `x` on top of `rest`, which we know about:
  | cons x rest ih =>
    -- open the last raise and the count of the head,
    simp only [bumpAll, bump, times]
    -- and split on whether the head is `c`.
    by_cases e : c = x
    -- It is:
    · -- the same fact, the other way round, for the count;
      have e' : x = c := e.symm
      -- both ifs add one, and the rest is the shorter case;
      rw [if_pos e, if_pos e', ih]
      -- then both sides are the same sum.
      omega
    -- It is not:
    · -- the same fact, the other way round, for the count;
      have e' : ¬ x = c := fun hx => e hx.symm
      -- both ifs add nothing, and the rest is the shorter case;
      rw [if_neg e, if_neg e', ih]
      -- then both sides are the same sum.
      omega

-- Promise: giving back each CPU of a list once subtracts, from each counter, how often the list
-- names it.
theorem giveBack_apply (c : Nat) (l : List Nat) :
    -- For any counters, after the give-backs CPU `c` reads its old value less its count in the list.
    ∀ counts : Counts, giveBack counts l c = counts c - times c l := by
  -- Go through the list one CPU at a time.
  induction l with
  -- Empty list:
  | nil =>
    -- whatever the counters,
    intro counts
    -- nothing is given back, and the count is zero.
    simp [giveBack, times]
  -- A head `x` on top of `rest`, which we know about:
  | cons x rest ih =>
    -- whatever the counters,
    intro counts
    -- the head is given back first, then the rest;
    simp only [giveBack, times]
    -- the rest is the shorter case, started from the counters with the head lowered;
    rw [ih (drop1 counts x)]
    -- open that lowering,
    simp only [drop1]
    -- and split on whether the head is `c`.
    by_cases e : c = x
    -- It is:
    · -- the same fact, the other way round, for the count;
      have e' : x = c := e.symm
      -- one less, then the rest, against one plus the rest;
      rw [if_pos e, if_pos e']
      -- subtracting one then the rest is subtracting both at once.
      omega
    -- It is not:
    · -- the same fact, the other way round, for the count;
      have e' : ¬ x = c := fun hx => e hx.symm
      -- nothing less, then the rest, against nothing plus the rest;
      rw [if_neg e, if_neg e']
      -- both sides are the same.
      omega

-- Promise: the occupancy of two lists of Holds put together is the sum of theirs.
theorem occupancy_append (c : Nat) (pre post : List (List Nat)) :
    -- Count `c` over `pre` then `post`: the count over `pre` plus the count over `post`.
    occupancy c (pre ++ post) = occupancy c pre + occupancy c post := by
  -- Go through `pre` one Hold at a time.
  induction pre with
  -- Empty `pre`: only `post` is left.
  | nil => simp [occupancy]
  -- A Hold `h` on top of `rest`, which we know about:
  | cons h rest ih =>
    -- put `h` in front, count it, and use the shorter case;
    simp only [List.cons_append, occupancy, ih]
    -- then both sides are the same sum.
    omega

-- Promise: a list that names CPU `c` counts it at least once.
theorem times_pos (c : Nat) (l : List Nat) (hc : c ∈ l) : 1 ≤ times c l := by
  -- Go through the list one CPU at a time.
  induction l with
  -- `c` is not in the empty list, so this case cannot happen.
  | nil => simp at hc
  -- A head `x` on top of `rest`:
  | cons x rest ih =>
    -- open the count of the head,
    simp only [times]
    -- `c` is the head or one of the rest;
    rw [List.mem_cons] at hc
    -- look at both.
    cases hc with
    -- It is the head:
    | inl e =>
      -- so the head counts one,
      rw [if_pos e.symm]
      -- and one plus anything is at least one.
      omega
    -- It is one of the rest:
    | inr hmem =>
      -- so the rest counts it at least once,
      have hrest := ih hmem
      -- and whatever the head counts, the sum is at least one.
      split <;> omega

-- Promise: a CPU that one of the Holds names has an occupancy of at least one.
theorem occupancy_pos (c : Nat) (holds : List (List Nat)) (mine : List Nat)
    -- Given a Hold `mine` among the Holds
    (hin : mine ∈ holds)
    -- that names `c`,
    (hc : c ∈ mine) :
    -- the Holds name `c` at least once.
    1 ≤ occupancy c holds := by
  -- Go through the Holds one at a time.
  induction holds with
  -- `mine` is not in the empty list, so this case cannot happen.
  | nil => simp at hin
  -- A Hold `g` on top of `rest`:
  | cons g rest ih =>
    -- open the occupancy of the first Hold,
    simp only [occupancy]
    -- `mine` is the first Hold or one of the rest;
    rw [List.mem_cons] at hin
    -- look at both.
    cases hin with
    -- It is the first:
    | inl e =>
      -- write the first Hold as `mine`,
      rw [← e]
      -- which names `c` at least once,
      have hmine := times_pos c mine hc
      -- so the sum is at least one.
      omega
    -- It is one of the rest:
    | inr hmem =>
      -- so the rest name `c` at least once,
      have hrest := ih hmem
      -- and so does the sum.
      omega

-- Promise: a share raises exactly the CPUs its Hold lists, and never fails.
theorem shareLoop_counts (size : Nat) (base : Counts) (cpus : List Nat) :
    -- For any record the loop starts with (its CPUs already raised on top of `base`),
    ∀ taken : List Nat, ∃ w : List Nat,
      -- the loop ends holding some list `w`, with exactly the CPUs of `w` raised on `base`.
      shareLoop size (bumpAll base taken) taken cpus = (bumpAll base w, w) := by
  -- Go through the CPUs one at a time.
  induction cpus with
  -- No CPU left:
  | nil =>
    -- whatever the record,
    intro taken
    -- the Hold is the record, and the counters carry it.
    exact ⟨taken, by simp [shareLoop]⟩
  -- A next CPU `cpu`, then `rest`:
  | cons cpu rest ih =>
    -- whatever the record,
    intro taken
    -- the CPU has a counter or not.
    by_cases h : cpu < size
    -- It has one:
    · -- it is raised and recorded, and the loop goes on;
      simp only [shareLoop, if_pos h]
      -- raising it on top of the record is the record with it added: the shorter case.
      exact ih (cpu :: taken)
    -- It has none:
    · -- it is skipped, and the loop goes on;
      simp only [shareLoop, if_neg h]
      -- with the same record: the shorter case.
      exact ih taken

-- Promise: a claim alone that wins keeps the books right, with its Hold added.
theorem hold_keeps_exact (size : Nat) (counts : Counts) (holds : List (List Nat))
    -- Given a claim over `cpus`, books that are right before it,
    (cpus : List Nat) (hexact : Exact counts holds)
    -- and the counters and Hold after it,
    (after : Counts) (won : List Nat)
    -- given that it won,
    (hwon : hold size counts cpus = (after, some won)) :
    -- the books are right after it, with its Hold among the others.
    Exact after (won :: holds) := by
  -- A won claim holds exactly the CPUs it asked for, each raised once.
  have hlist := hold_won_list size counts cpus after won hwon
  -- Name the Hold and the counters by what they are.
  rw [hlist.1, hlist.2]
  -- Open what "the books are right" means,
  unfold Exact
  -- and check one CPU `c`.
  intro c
  -- Its counter is its old value plus its count in the claim, and its old value was right;
  rw [bumpAll_apply, hexact c]
  -- the occupancy now counts the new Hold first;
  simp only [occupancy]
  -- both sides are the same sum.
  omega

-- Promise: a share keeps the books right, with its Hold added.
theorem share_keeps_exact (size : Nat) (counts : Counts) (holds : List (List Nat))
    -- Given a share over `cpus` and books that are right before it,
    (cpus : List Nat) (hexact : Exact counts holds) :
    -- the share returns some Hold `w` with exactly its CPUs raised,
    ∃ w : List Nat, share size counts cpus = (bumpAll counts w, w)
      -- and the books are right after it, with `w` among the others.
      ∧ Exact (bumpAll counts w) (w :: holds) := by
  -- A share raises exactly the CPUs its Hold lists.
  cases shareLoop_counts size counts cpus [] with
  -- Call that Hold `w`.
  | intro w hw =>
    -- It is the Hold we name; the share's result is that one;
    refine ⟨w, hw, ?_⟩
    -- open what "the books are right" means,
    unfold Exact
    -- and check one CPU `c`.
    intro c
    -- Its counter is its old value plus its count in `w`, and its old value was right;
    rw [bumpAll_apply, hexact c]
    -- the occupancy now counts `w` first;
    simp only [occupancy]
    -- both sides are the same sum.
    omega

-- Promise: dropping a Hold keeps the books right, with that Hold gone.
theorem release_keeps_exact (counts : Counts) (pre post : List (List Nat)) (gone : List Nat)
    -- Given books that are right with the Hold `gone` somewhere among the others,
    (hexact : Exact counts (pre ++ gone :: post)) :
    -- after `gone` gives each of its CPUs back once, the books are right without it.
    Exact (giveBack counts gone) (pre ++ post) := by
  -- Open what "the books are right" means,
  unfold Exact
  -- and check one CPU `c`.
  intro c
  -- Its counter is its old value less its count in `gone`; the old value was right; split both
  -- occupancies into the part before `gone` and the part from `gone` on.
  rw [giveBack_apply, hexact c, occupancy_append, occupancy_append]
  -- The part from `gone` on counts `gone` first.
  simp only [occupancy]
  -- Taking `gone`'s count back out leaves the others' counts.
  omega

-- Promise: when the books are right, a CPU that some Hold lists never reads free.
theorem held_never_free (counts : Counts) (holds : List (List Nat))
    -- Given books that are right,
    (hexact : Exact counts holds)
    -- a Hold `mine` among them,
    (mine : List Nat) (hin : mine ∈ holds)
    -- and a CPU `c` it lists,
    (c : Nat) (hc : c ∈ mine) :
    -- that CPU's counter is at least one.
    1 ≤ counts c := by
  -- The counter is the occupancy,
  rw [hexact c]
  -- and the occupancy of a CPU some Hold names is at least one.
  exact occupancy_pos c holds mine hin hc

end CpuLedger
