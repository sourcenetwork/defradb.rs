-- Exact books, shares and give-backs come from CpuLedger.Count.
import CpuLedger.Count

/-!
# CpuLedger.Exclusive: pools that claimed alone never hold one CPU together

The story, in plain words. The Holds of the live pools are of two kinds: the
ones claimed alone (Ledger::hold) and the ones shared (Ledger::share). A
shared Hold may sit on CPUs another pool holds, alone or shared; that is what
sharing means, and its report says so. A Hold claimed alone must not: no CPU
may be in two of them, and a claim alone must not win a CPU any Hold lists,
shared ones included, or a pool that was promised its own cores would land
next to another.

A tiny example: pool A holds CPU 0 alone, pool B shares CPUs 0 and 1. The
counters read 2 and 1, so a claim alone of CPU 1 fails (CPU 1 reads 1) and a
claim alone of CPU 2 wins. When A goes, CPU 0 still reads 1 because B is
there, so a claim alone of CPU 0 still fails.

What must never happen:
* a claim alone wins a CPU that any Hold lists (`hold_won_alone`);
* two Holds claimed alone list one CPU (`Apart`, kept by `hold_step`,
  `share_step`, `release_alone_step` and `release_shared_step`: every
  operation on the ledger keeps it, and keeps the books exact).

These laws hold for any number of CPUs and pools. The races between pools
are the TLA+ model `proofs/tla/CpuLedger.tla`.

Rust anchors: `Ledger::hold`, `Ledger::share`, `Hold::drop`
(`crates/workers/src/ledger.rs`) and `reserve` (`crates/workers/src/placement/reserve.rs`).
-/

namespace CpuLedger

-- Promise: when the books are right, a claim alone that wins took no CPU any Hold lists.
theorem hold_won_alone (size : Nat) (counts : Counts) (holds : List (List Nat))
    -- Given a claim over `cpus` and books that are right before it,
    (cpus : List Nat) (hexact : Exact counts holds)
    -- the counters and Hold after it,
    (after : Counts) (won : List Nat)
    -- and given that it won,
    (hwon : hold size counts cpus = (after, some won)) :
    -- no CPU of its Hold is in any Hold that was there before.
    ∀ c ∈ won, ∀ g ∈ holds, c ∉ g := by
  -- Take a CPU `c` of the new Hold, an old Hold `g`, and suppose `g` lists `c`.
  intro c hcw g hg hcg
  -- The new Hold lists exactly the CPUs asked for,
  have hlist := (hold_won_list size counts cpus after won hwon).1
  -- so `c` is one of the CPUs the claim asked for.
  rw [hlist, List.mem_reverse] at hcw
  -- A won claim found each of them at zero, `c` included;
  have hfree := (hold_won_free size counts cpus after won hwon c hcw).2
  -- but `g` lists `c`, so with the books right its counter is at least one.
  have hpos := held_never_free counts holds hexact g hg c hcg
  -- Zero and at least one cannot both hold.
  omega

-- No CPU is in two Holds of the list: each Hold shares no CPU with any Hold after it.
def Apart : List (List Nat) → Prop
  -- No Holds: nothing to collide.
  | [] => True
  -- The first Hold is apart from every later one, and the later ones are apart among themselves.
  | h :: rest => (∀ c ∈ h, ∀ g ∈ rest, c ∉ g) ∧ Apart rest

-- Promise: a claim alone that wins keeps the books right and the Holds claimed alone apart.
theorem hold_step (size : Nat) (counts : Counts) (alone shared : List (List Nat))
    -- Given a claim over `cpus`, books right over both kinds of Holds,
    (cpus : List Nat) (hexact : Exact counts (alone ++ shared))
    -- and the Holds claimed alone apart,
    (hapart : Apart alone)
    -- the counters and Hold after the claim,
    (after : Counts) (won : List Nat)
    -- and given that it won,
    (hwon : hold size counts cpus = (after, some won)) :
    -- the books are right with the new Hold among those claimed alone, and those are still apart.
    Exact after ((won :: alone) ++ shared) ∧ Apart (won :: alone) := by
  -- Prove the two halves one after the other.
  constructor
  -- The books:
  · -- putting the new Hold in front of both kinds is the same list,
    rw [List.cons_append]
    -- and a won claim keeps the books right with its Hold in front.
    exact hold_keeps_exact size counts (alone ++ shared) cpus hexact after won hwon
  -- Apart:
  · -- open what apart means for a list with the new Hold in front,
    simp only [Apart]
    -- the old ones were apart already, so only the new Hold is left to check;
    refine ⟨?_, hapart⟩
    -- take a CPU `c` of the new Hold and a Hold `g` claimed alone before it;
    intro c hcw g hg
    -- `g` is among all the Holds, and the new Hold took no CPU any of them lists.
    exact hold_won_alone size counts (alone ++ shared) cpus hexact after won hwon c hcw g
      (List.mem_append.mpr (Or.inl hg))

-- Promise: a share keeps the books right, its Hold added to the shared ones; the Holds claimed
-- alone are untouched, so they stay apart.
theorem share_step (size : Nat) (counts : Counts) (alone shared : List (List Nat))
    -- Given a share over `cpus` and books right over both kinds of Holds,
    (cpus : List Nat) (hexact : Exact counts (alone ++ shared)) :
    -- the share returns a Hold `w` with exactly its CPUs raised,
    ∃ w : List Nat, share size counts cpus = (bumpAll counts w, w)
      -- and the books are right with `w` among the shared Holds.
      ∧ Exact (bumpAll counts w) (alone ++ w :: shared) := by
  -- A share keeps the books right with its Hold in front of all the others.
  cases share_keeps_exact size counts (alone ++ shared) cpus hexact with
  -- Call that Hold `w`.
  | intro w hw =>
    -- It is the Hold we name, and the share's result is that one;
    refine ⟨w, hw.1, ?_⟩
    -- open what "the books are right" means,
    unfold Exact
    -- and check one CPU `c`.
    intro c
    -- With `w` in front, the counter is `w`'s count plus the others';
    have hc := hw.2 c
    -- count `w` first,
    simp only [occupancy] at hc
    -- then split the others into those claimed alone and those shared.
    rw [occupancy_append] at hc
    -- On our side, split at the end of the Holds claimed alone,
    rw [occupancy_append]
    -- and count `w` first among the shared ones.
    simp only [occupancy]
    -- Both sides are the same sum, in another order.
    omega

-- Promise: removing one Hold from a list of Holds that are apart leaves them apart.
theorem release_keeps_apart (gone : List Nat) (post : List (List Nat)) :
    -- For any part `pre` before it,
    ∀ pre : List (List Nat), Apart (pre ++ gone :: post) → Apart (pre ++ post) := by
  -- Name the part before it,
  intro pre
  -- and go through it one Hold at a time.
  induction pre with
  -- Nothing before it:
  | nil =>
    -- name the fact that `gone` and `post` are apart;
    intro hapart
    -- `gone` is first, so the list after it is apart;
    simp only [List.nil_append, Apart] at hapart
    -- our list is just `post`;
    simp only [List.nil_append]
    -- which is the second half of what we have.
    exact hapart.2
  -- A Hold `g` before the rest of `pre`, which we know about:
  | cons g rest ih =>
    -- name the fact that the whole list is apart;
    intro hapart
    -- `g` is first: it is apart from everything after it, and the rest is apart;
    simp only [List.cons_append, Apart] at hapart
    -- our list also has `g` first;
    simp only [List.cons_append, Apart]
    -- the rest without `gone` is apart by the shorter case; `g` is left to check;
    refine ⟨?_, ih hapart.2⟩
    -- take a CPU `c` of `g` and a Hold `k` after it in our list;
    intro c hc k hk
    -- `k` was after `g` in the list we were given, too:
    have hk' : k ∈ rest ++ gone :: post := by
      -- `k` is in the rest of `pre` or in `post`,
      rw [List.mem_append] at hk
      -- and the given list is the rest of `pre`, then `gone`, then `post`;
      rw [List.mem_append, List.mem_cons]
      -- look at both places `k` can be.
      cases hk with
      -- In the rest of `pre`: the first place.
      | inl hrest => exact Or.inl hrest
      -- In `post`: the last place.
      | inr hpost => exact Or.inr (Or.inr hpost)
    -- so `g` and `k` share no CPU.
    exact hapart.1 c hc k hk'

-- Promise: dropping a Hold claimed alone keeps the books right and the rest apart.
theorem release_alone_step (counts : Counts) (pre post shared : List (List Nat))
    -- Given a Hold `gone` claimed alone, between `pre` and `post`,
    (gone : List Nat)
    -- books right over both kinds of Holds,
    (hexact : Exact counts ((pre ++ gone :: post) ++ shared))
    -- and the Holds claimed alone apart,
    (hapart : Apart (pre ++ gone :: post)) :
    -- after `gone` gives its CPUs back, the books are right without it and the rest stay apart.
    Exact (giveBack counts gone) ((pre ++ post) ++ shared) ∧ Apart (pre ++ post) := by
  -- Prove the two halves one after the other.
  constructor
  -- The books:
  · -- regroup our list as `pre`, then the rest,
    rw [List.append_assoc]
    -- and regroup the list we were given the same way, with `gone` at the front of the rest;
    rw [List.append_assoc, List.cons_append] at hexact
    -- dropping a Hold keeps the books right with it gone.
    exact release_keeps_exact counts pre (post ++ shared) gone hexact
  -- Apart:
  · -- removing a Hold from Holds that were apart leaves them apart.
    exact release_keeps_apart gone post pre hapart

-- Promise: dropping a shared Hold keeps the books right; the Holds claimed alone are untouched,
-- so they stay apart.
theorem release_shared_step (counts : Counts) (alone pre post : List (List Nat))
    -- Given a shared Hold `gone`, between `pre` and `post` among the shared ones,
    (gone : List Nat)
    -- and books right over both kinds of Holds,
    (hexact : Exact counts (alone ++ (pre ++ gone :: post))) :
    -- after `gone` gives its CPUs back, the books are right without it.
    Exact (giveBack counts gone) (alone ++ (pre ++ post)) := by
  -- Regroup the list we were given as the Holds before `gone`, then `gone`, then `post`;
  rw [← List.append_assoc] at hexact
  -- and regroup ours the same way, without `gone`;
  rw [← List.append_assoc]
  -- dropping a Hold keeps the books right with it gone.
  exact release_keeps_exact counts (alone ++ pre) post gone hexact

end CpuLedger
