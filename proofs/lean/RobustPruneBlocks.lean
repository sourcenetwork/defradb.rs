/-!
# RobustPruneBlocks: checking candidates in blocks keeps the same edges

The story, in plain words. RobustPrune picks a node's out-edges. It walks the
candidates nearest first and keeps one unless an edge it already kept covers
it, and it stops once it has kept `max`. Checking one candidate against every
kept edge, one candidate at a time, reads each kept edge again for every
candidate, which is most of a DiskANN merge.

So the code checks candidates in blocks. For each block it first works out,
for every candidate in it, whether an edge kept before the block covers it,
reading each of those edges once for the whole block. Then it walks the block
in order, and a candidate the earlier edges did not cover is checked only
against the edges kept inside the block so far.

A tiny example. Kept so far: [e]. Block: [c1, c2]. First: does e cover c1?
does e cover c2? Then c1: covered by e, or by nothing kept in this block yet.
If c1 is kept, c2 is checked against e (already known) and against c1.

What must never happen: the blocked walk keeps a different list of edges
than the one-at-a-time walk, for any cut of the candidates into blocks
(`blocked_eq_sequential`). A walk that skips the in-block check keeps a covered
edge (`no_since_keeps_a_covered_edge`).

What covers what is left abstract (`covers`), so the result holds whatever
distance and `alpha` decide it.

Rust anchors: `RobustPrune::select`
(`crates/db/src/index/vector/engine/select/robust_prune.rs`), whose blocks are
`pool.chunks(BLOCK)`; `squared_euclidean_block`
(`crates/defra-core/src/vector/kernel/mod.rs`) computes the distances behind
the before-the-block answers, bit-equal per pair to `squared_euclidean`
(`crates/defra-core/tests/vector_kernel.rs` `a_block_is_bit_equal_to_each_pair`).
The Rust loop over earlier edges stops once every candidate of the block is
covered; an answer that is already "covered" stays so, which is why the full
`any` below is the same answer.
-/

namespace RobustPruneBlocks

variable {α : Type} (covers : α → α → Bool) (max : Nat)

-- One candidate, one at a time: kept unchanged once `max` are kept or when a
-- kept edge covers it; otherwise it joins the end of the kept list.
def step (kept : List α) (c : α) : List α :=
  if max ≤ kept.length then kept
  else if kept.any (fun e => covers e c) then kept
  else kept ++ [c]

-- RobustPrune as the paper writes it: every candidate in order, from nothing.
def sequential (candidates : List α) : List α :=
  candidates.foldl (step covers max) []

-- One candidate inside a block that began with `before` kept. `flag` is the
-- precomputed answer for the edges in `before`; only the edges kept since
-- the block began (`kept.drop before.length`) are checked now.
def inner (before : List α) (kept : List α) (cf : α × Bool) : List α :=
  if max ≤ kept.length then kept
  else if cf.2 || (kept.drop before.length).any (fun e => covers e cf.1) then kept
  else kept ++ [cf.1]

-- One block: nothing to do once `max` are kept; otherwise answer "covered by
-- an edge kept before the block?" for every candidate, then walk the block.
def blockStep (before : List α) (block : List α) : List α :=
  if max ≤ before.length then before
  else (block.map (fun c => (c, before.any (fun e => covers e c)))).foldl
    (inner covers max before) before

-- RobustPrune in blocks: every block in order, from nothing.
def blocked (blocks : List (List α)) : List α :=
  blocks.foldl (blockStep covers max) []

-- A step only ever adds to the end of the kept list.
theorem step_extends (kept : List α) (c : α) :
    ∃ s, step covers max kept c = kept ++ s := by
  unfold step
  -- each branch keeps the list or appends the candidate
  split
  · exact ⟨[], by simp⟩
  · split
    · exact ⟨[], by simp⟩
    · exact ⟨[c], rfl⟩

-- Inside a block, a candidate's step is the one-at-a-time step: the kept list
-- is the edges from before the block plus those kept since, and "covered by
-- any of them" is "covered by one from before, or by one since".
theorem inner_eq_step (before s : List α) (c : α) :
    inner covers max before (before ++ s) (c, before.any (fun e => covers e c))
      = step covers max (before ++ s) c := by
  -- dropping the edges from before leaves exactly those kept since
  have hdrop : (before ++ s).drop before.length = s := List.drop_left before s
  simp only [inner, step, hdrop, List.any_append]

-- Walking a whole block with precomputed answers is walking it one at a
-- time, from any kept list that begins with the block's starting edges.
theorem fold_inner (before : List α) (block : List α) :
    ∀ s, (block.map (fun c => (c, before.any (fun e => covers e c)))).foldl
        (inner covers max before) (before ++ s)
      = block.foldl (step covers max) (before ++ s) := by
  induction block with
  | nil => intro s; rfl
  | cons c rest ih =>
    intro s
    -- the first candidate's step keeps `before` as the start of the list
    obtain ⟨t, ht⟩ := step_extends covers max (before ++ s) c
    simp only [List.map_cons, List.foldl_cons]
    rw [inner_eq_step, ht, List.append_assoc, ih (s ++ t), ← List.append_assoc, ← ht]

-- Once `max` edges are kept, no candidate changes anything.
theorem fold_full (block : List α) :
    ∀ kept, max ≤ kept.length → block.foldl (step covers max) kept = kept := by
  induction block with
  | nil => intro kept _; rfl
  | cons c rest ih =>
    intro kept h
    simp only [List.foldl_cons, step, h, if_true]
    exact ih kept h

-- One block of the blocked walk is that block walked one at a time.
theorem blockStep_eq (before block : List α) :
    blockStep covers max before block = block.foldl (step covers max) before := by
  unfold blockStep
  split
  · rename_i h
    exact (fold_full covers max block before h).symm
  · -- start the walk from `before ++ []`, which is `before`
    have h := fold_inner covers max before block []
    simp only [List.append_nil] at h
    exact h

-- However the candidates are cut into blocks, the blocked walk keeps exactly
-- the edges the one-at-a-time walk keeps.
theorem blocked_eq_sequential (blocks : List (List α)) :
    blocked covers max blocks = sequential covers max blocks.flatten := by
  unfold blocked sequential
  rw [List.foldl_flatten]
  -- each block's step is that block walked one candidate at a time
  congr 1
  funext before block
  exact blockStep_eq covers max before block

-- The bug the in-block check prevents: a block walk that trusts only the
-- answers from before the block.
def blockStepNoSince (before : List α) (block : List α) : List α :=
  if max ≤ before.length then before
  else block.foldl (fun kept c =>
    if max ≤ kept.length then kept
    else if before.any (fun e => covers e c) then kept
    else kept ++ [c]) before

-- Without it the law fails: two copies of one vector in one block are both
-- kept, though the first covers the second.
theorem no_since_keeps_a_covered_edge :
    [[0, 0]].foldl (blockStepNoSince (fun e c : Nat => e == c) 64) [] = [0, 0] ∧
    sequential (fun e c : Nat => e == c) 64 [0, 0] = [0] := by
  decide

end RobustPruneBlocks
