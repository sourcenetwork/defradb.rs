/-!
# HistoryWindow.Window: which commits a history prune deletes

The story, in plain words. A collection declares `@retention(maxDepth: n)`.
When its history is pruned, each document looks at its highest composite
commit, at height `top`, and deletes every composite whose height is at most
`top - n`, unless that composite is a live head. Heights start at 1. Rust
computes `top - n` with `saturating_sub`, which is Lean's subtraction on
naturals: it stops at 0.

What must never happen:
* a live head is deleted (`head_kept`);
* a commit within `n` heights of the top is deleted (`window_kept`);
* the saturating subtraction deletes a commit when the history is shorter
  than `n` (`short_history_kept`).
What must happen: a commit that is not a head and lies `n` or more heights
below the top is deleted (`below_pruned`), so the prune does its job.
And, while the head flag stays fixed, a decision made from an older, lower
`top` deletes nothing the newer `top` would keep (`stale_top_safe`). Transaction
safety when an update changes the head flag belongs to the temporal model.

Rust anchor: `prune_doc_in` in
`crates/database/db/src/collection/ops/prune_history.rs`.
-/

namespace HistoryWindow

-- Whether the prune deletes a composite at height `p`, given the highest
-- height `top`, the depth `n`, and whether the composite is a live head:
-- not a head, and at or below the cutoff `top - n`.
def pruned (top n p : Nat) (head : Bool) : Bool :=
  !head && decide (p ≤ top - n)

-- A live head is never deleted, whatever its height.
theorem head_kept (top n p : Nat) : pruned top n p true = false := by
  -- `!true` is `false`, and `false && _` is `false`.
  simp [pruned]

-- A commit within `n` heights of the top (its height plus `n` exceeds the
-- top) is never deleted.
theorem window_kept (top n p : Nat) (head : Bool) (hp : 1 ≤ p) (hw : top < p + n) :
    pruned top n p head = false := by
  -- The cutoff `top - n` is below `p`, so the height test fails.
  have : ¬ p ≤ top - n := by omega
  simp [pruned, this]

-- When the history is no taller than `n`, nothing is deleted: the cutoff
-- saturates at 0 and every height is at least 1.
theorem short_history_kept (top n p : Nat) (head : Bool) (hp : 1 ≤ p) (hs : top ≤ n) :
    pruned top n p head = false := by
  -- `top - n` is 0 here, and `p` is at least 1.
  have : ¬ p ≤ top - n := by omega
  simp [pruned, this]

-- A commit that is not a head and lies `n` or more heights below the top
-- is deleted.
theorem below_pruned (top n p : Nat) (hb : p + n ≤ top) : pruned top n p false = true := by
  -- The height test holds, and `!false` is `true`.
  have : p ≤ top - n := by omega
  simp [pruned, this]

-- A decision from an older, lower top deletes only what the newer top
-- would delete too.
theorem stale_top_safe (top top' n p : Nat) (head : Bool) (hle : top ≤ top')
    (h : pruned top n p head = true) : pruned top' n p head = true := by
  -- Unfold both decisions: the head flag is the same, and the cutoff only grows.
  simp only [pruned, Bool.and_eq_true, Bool.not_eq_true', decide_eq_true_eq] at *
  -- The head part carries over; the height part follows from `top ≤ top'`.
  exact ⟨h.1, by omega⟩

end HistoryWindow
