/-!
# WalkCutoff: skipping neighbours ranked past the frontier keeps the frontier

The story, in plain words. A DiskANN walk keeps a frontier: the `l` nearest
nodes it has seen, nearest first. Each step expands one node, ranks every
neighbour it has not seen, adds them to the frontier, sorts it and keeps the
first `l`. Copying a neighbour's code costs time, so the code now skips a
neighbour that ranks after the frontier's last place before the step: once
the frontier is full, such a neighbour can only land past place `l`, where
the truncate drops it anyway.

A tiny example. `l = 2`, frontier [3, 5], new neighbours 4 and 9. Without the
cutoff: [3, 4, 5, 9], keep [3, 4]. With it, 9 ranks after 5 and is skipped:
[3, 4, 5], keep [3, 4]. Same frontier.

What must never happen: the frontier after a step with the cutoff differs
from the frontier without it (`cut_step_eq_step`). The rank compares the
distance and then the node id, the order the frontier is sorted by; a cutoff
that compares only the distance drops a neighbour that ties the last place
and wins on its id (`distance_only_cutoff_drops_a_tie`).

A rank is a number here: the position of (distance, id) in the frontier's
order. Ids in a walk are distinct, so no two ranks are equal, and sorting a
list is the same as inserting its items one by one into a sorted list.

Rust anchors: `DiskAnn::walk` and `DiskAnn::each_edge`
(`crates/db/src/index/vector/engine/diskann/graph.rs`), sorted by
`by_distance` in the same file.
-/

namespace WalkCutoff

-- Insert a rank into a sorted list, after every rank not above it.
def ins (x : Nat) : List Nat → List Nat
  -- an empty list just holds the new rank
  | [] => [x]
  -- the new rank goes first only if it is below the head
  | y :: ys => if x < y then x :: y :: ys else y :: ins x ys

-- Add every new rank to the frontier, one by one: append and sort.
def addAll (g : List Nat) (adds : List Nat) : List Nat :=
  adds.foldl (fun acc x => ins x acc) g

-- The rank at place `n`, counting from 0; 0 past the end.
def get0 : List Nat → Nat → Nat
  -- past the end
  | [], _ => 0
  -- place 0 is the head
  | a :: _, 0 => a
  -- place n+1 is place n of the tail
  | _ :: r, n + 1 => get0 r n

-- One step without the cutoff: add every neighbour, keep the first `l`.
def step (l : Nat) (f adds : List Nat) : List Nat :=
  (addAll f adds).take l

-- One step with the cutoff: once the frontier holds `l`, only neighbours
-- ranked before its last place are added.
def cutStep (l : Nat) (f adds : List Nat) : List Nat :=
  -- a full frontier: skip everything ranked after place l - 1
  if 0 < l ∧ l ≤ f.length then
    (addAll f (adds.filter (fun x => x < get0 f (l - 1)))).take l
  -- not full yet: every neighbour can still make it
  else step l f adds

-- Cutting the tail to `n` first does not change the first `n`.
theorem take_cons_take (y : Nat) (ys : List Nat) :
    ∀ n : Nat, (y :: ys).take n = (y :: ys.take n).take n
  -- keeping nothing
  | 0 => rfl
  -- the head, then the first `k` of a tail already cut to `k + 1`
  | k + 1 => by simp [List.take_take, Nat.min_eq_left (Nat.le_succ k)]

-- The first `l` after an insert depend only on the first `l` before it.
theorem take_ins_take (x : Nat) :
    ∀ (g : List Nat) (l : Nat), (ins x g).take l = (ins x (g.take l)).take l
  -- an empty list: both sides insert into nothing
  | [], l => by simp [ins]
  -- keeping nothing: both sides are empty
  | _ :: _, 0 => by simp
  -- the new rank goes first, or the head stays and the rest recurses
  | y :: ys, n + 1 => by
    simp only [List.take_succ_cons, ins]
    split
    · simp [take_cons_take y ys n]
    · simp [take_ins_take x ys n]

-- A rank at or after every one of the first `l` leaves them alone.
theorem take_ins_above (x : Nat) :
    ∀ (g : List Nat) (l : Nat), l ≤ g.length → (∀ y ∈ g.take l, y ≤ x) →
      (ins x g).take l = g.take l
  -- keeping nothing changes nothing
  | _, 0, _, _ => by simp
  -- an empty list cannot hold `l + 1` ranks
  | [], _ + 1, h, _ => by simp at h
  -- the head is at most x, so x goes past it and the rest recurses
  | y :: ys, n + 1, h, hx => by
    have hy : ¬ x < y := Nat.not_lt.mpr (hx y (by simp))
    simp only [ins, hy, if_false, List.take_succ_cons]
    have ih := take_ins_above x ys n (by simp at h; omega)
      (fun z hz => hx z (by simp [hz]))
    rw [ih]

-- After an insert, each of the first `l` is the new rank or was there before.
theorem mem_take_ins (x z : Nat) :
    ∀ (g : List Nat) (l : Nat), z ∈ (ins x g).take l → z = x ∨ z ∈ g.take l
  -- an empty list: only the new rank is there
  | [], l, h => by
    left
    simpa [ins] using List.mem_of_mem_take h
  -- keeping nothing holds nothing
  | _ :: _, 0, h => by simp at h
  -- the new rank first, or the head first and the rest recurses
  | y :: ys, n + 1, h => by
    simp only [ins] at h
    split at h
    · simp only [List.take_succ_cons, List.mem_cons] at h
      rcases h with h | h
      · exact Or.inl h
      · right
        rw [take_cons_take y ys n] at h
        simpa using List.mem_of_mem_take h
    · simp only [List.take_succ_cons, List.mem_cons] at h ⊢
      rcases h with h | h
      · exact Or.inr (Or.inl h)
      · rcases mem_take_ins x z ys n h with h | h
        · exact Or.inl h
        · exact Or.inr (Or.inr h)

-- An insert adds one rank.
theorem length_ins (x : Nat) : ∀ g : List Nat, (ins x g).length = g.length + 1
  -- into nothing
  | [] => rfl
  -- first, or after the head
  | y :: ys => by
    unfold ins
    split <;> simp [length_ins x ys]

-- The heart of it: from frontiers that agree on their first `l`, and whose
-- first `l` are all at most `last`, adding every rank and adding only those
-- below `last` keep the same first `l`.
theorem cut_keeps (l last : Nat) :
    ∀ (adds g1 g2 : List Nat), g1.take l = g2.take l → l ≤ g1.length →
      (∀ y ∈ g1.take l, y ≤ last) →
      (addAll g1 adds).take l = (addAll g2 (adds.filter (fun x => x < last))).take l
  -- nothing to add: the frontiers already agree
  | [], _, _, h, _, _ => by simpa [addAll] using h
  | x :: rest, g1, g2, h, hl, hb => by
    by_cases hx : x < last
    -- a rank below `last` is added on both sides
    · have hf : (x :: rest).filter (fun x => x < last) =
          x :: rest.filter (fun x => x < last) := by simp [hx]
      simp only [addAll, hf, List.foldl_cons]
      apply cut_keeps l last rest (ins x g1) (ins x g2)
      · rw [take_ins_take x g1, take_ins_take x g2, h]
      · rw [length_ins]; omega
      · intro y hy
        rcases mem_take_ins x y g1 l hy with hy | hy
        · omega
        · exact hb y hy
    -- a rank at or above `last` is skipped, and without the cutoff it lands
    -- past place `l`
    · have hf : (x :: rest).filter (fun x => x < last) =
          rest.filter (fun x => x < last) := by simp [hx]
      have hk : (ins x g1).take l = g1.take l :=
        take_ins_above x g1 l hl (fun y hy => by have := hb y hy; omega)
      simp only [addAll, hf, List.foldl_cons]
      apply cut_keeps l last rest (ins x g1) g2
      · rw [hk, h]
      · rw [length_ins]; omega
      · rw [hk]; exact hb

-- The rank at a place inside a list is one of its ranks.
theorem get0_mem : ∀ (f : List Nat) (n : Nat), n < f.length → get0 f n ∈ f
  -- an empty list has no places
  | [], _, h => by simp at h
  -- place 0 is the head
  | a :: _, 0, _ => by simp [get0]
  -- place n+1 is in the tail
  | _ :: r, n + 1, h => by
    simp only [get0, List.mem_cons]
    exact Or.inr (get0_mem r n (by simp at h; omega))

-- In a sorted list, each of the first `l` is at most the rank at place l - 1.
theorem take_le_last :
    ∀ (f : List Nat) (l : Nat), f.Pairwise (· ≤ ·) → 0 < l → l ≤ f.length →
      ∀ y ∈ f.take l, y ≤ get0 f (l - 1)
  -- an empty list holds no `l` ranks
  | [], _, _, _, h => by simp at h; omega
  -- keeping nothing is ruled out
  | _ :: _, 0, _, h, _ => by omega
  | a :: r, n + 1, hs, _, hl => by
    intro y hy
    simp only [List.take_succ_cons, List.mem_cons] at hy
    have hs' := List.pairwise_cons.mp hs
    cases n with
    -- keeping one: only the head, which is the last place
    | zero => simp at hy; simp [get0, hy]
    -- the head is at most everything after it; the rest recurses
    | succ k =>
      simp only [Nat.add_sub_cancel, get0]
      rcases hy with hy | hy
      · rw [hy]
        exact hs'.1 _ (get0_mem r k (by simp at hl; omega))
      · exact take_le_last r (k + 1) hs'.2 (by omega) (by simp at hl; omega) y hy

-- Every step: from a sorted frontier, the step with the cutoff keeps the
-- same frontier as the step without it.
theorem cut_step_eq_step (l : Nat) (f adds : List Nat) (hs : f.Pairwise (· ≤ ·)) :
    cutStep l f adds = step l f adds := by
  unfold cutStep step
  split
  -- a full frontier: its first `l` are at most its last place
  · rename_i h
    exact (cut_keeps l (get0 f (l - 1)) adds f f rfl h.2
      (take_le_last f l hs h.1 h.2)).symm
  -- not full: nothing is skipped
  · rfl

-- The planted bug: rank = 10 * distance + id, and the cutoff compares only
-- the distance (rank / 10), so a neighbour at the last place's distance is
-- skipped even when its smaller id ranks it first.
def distanceOnlyStep (l : Nat) (f adds : List Nat) : List Nat :=
  -- the full-frontier case, with the coarser comparison
  if 0 < l ∧ l ≤ f.length then
    (addAll f (adds.filter (fun x => x / 10 < get0 f (l - 1) / 10))).take l
  else step l f adds

-- Frontier [distance 1, id 2], `l = 1`, a new neighbour at distance 1, id 1:
-- it should take the only place, and the distance-only cutoff drops it.
theorem distance_only_cutoff_drops_a_tie :
    distanceOnlyStep 1 [12] [11] ≠ step 1 [12] [11] := by
  decide

end WalkCutoff
