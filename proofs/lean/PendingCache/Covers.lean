/-!
# PendingCache.Covers: an insert-only log, a cache of it, and a merge that cuts it

The story, in plain words. A DiskANN index keeps the vectors it has not
linked into its graph yet as a commit-ordered log of inserts, and nothing
else: what is pending at a snapshot is every vector the log's live entries
insert, up to the snapshot's head. A delete appends nothing; a deleted
vector stays a candidate, which a search drops when it ranks.

Each node keeps a copy of the log in memory: loaded once, then caught up by
adding what later entries insert. Within one generation the log only grows,
so a snapshot of that generation sees a prefix of it.

A merge cannot always link everything at once, so it links the live vectors
a prefix of the log inserts and truncates the log through that prefix. What
stays pending afterwards is what the rest of the log inserts.

A tiny example. The log holds "insert 1", "insert 2". A copy loaded after
the first entry holds {1}; caught up, it holds {1, 2}. A snapshot after the
first entry has {1} pending, which the copy holds. A merge that cuts after
the first entry links 1 and leaves "insert 2", so 2 is still pending.

What must never happen:
* a snapshot whose log is a prefix of what the copy has seen has a vector
  pending that the copy lacks (`prefix_covered`);
* catching up in two steps gives a different copy than catching up in one
  (`catch_up_in_steps`);
* a live vector pending before a merge is neither linked by it nor still
  pending after it, wherever it cut (`merge_keeps_every_vector`);
* and the law does not survive truncating through the end of the log while
  linking only a prefix, which is why the merge truncates through what it
  linked (`truncate_through_head_loses`).

The order of commits, the generation tag and a transaction's own appends are
the TLA+ model `proofs/tla/PendingCache.tla`; this file proves the laws it
rests on for logs of any length.

Rust anchors: `PendingCache` and its catch-up
(`crates/db/src/index/vector/engine/diskann/pending_cache.rs`), the append in
`insert` (`crates/db/src/index/vector/engine/diskann/mod.rs`), and
`merge_plan`, `merge` and `truncate_pending_log`
(`crates/db/src/index/vector/engine/diskann/merge.rs`).
-/

namespace PendingCache

-- A run of log entries: each one the id of the vector it inserts, oldest first.
abbrev Log := List Nat

-- The copy after a load of `loaded` and a catch-up over `later`: every vector either inserts.
def copy (loaded later : Log) : Nat → Prop :=
  -- Loaded, or inserted since.
  fun x => x ∈ loaded ∨ x ∈ later

-- Promise: a copy loaded from `seen` and caught up over `later` holds every vector pending at a snapshot whose log is any prefix of `seen ++ later`.
theorem prefix_covered (snap seen later : Log) (x : Nat)
    (h : snap <+: seen ++ later) :
    -- Pending at that snapshot means: in the copy.
    x ∈ snap → copy seen later x := by
  -- `x` is pending at the snapshot.
  intro hx
  -- The snapshot's log is a prefix of the copy's, so `x` is in the copy's log.
  have hall : x ∈ seen ++ later := h.subset hx
  -- That log is `seen` then `later`: `x` is in one of them.
  exact List.mem_append.mp hall

-- Promise: catching up over `a`, then over `b`, gives the copy that catching up over `a ++ b` at once gives.
theorem catch_up_in_steps (loaded a b : Log) (x : Nat) :
    -- The copy after two catch-ups is the copy after one.
    copy (loaded ++ a) b x ↔ copy loaded (a ++ b) x := by
  -- Unfold the copies into "loaded, or in a, or in b".
  simp only [copy, List.mem_append]
  -- Both sides say the same thing, grouped differently.
  exact or_assoc

-- Promise: a merge that links the live vectors a prefix of the log inserts, and keeps the rest of the log, loses no live vector.
theorem merge_keeps_every_vector (pre post : Log) (live : Nat → Prop) (x : Nat)
    (hlive : live x) :
    -- Pending before the merge means: linked by it, or pending after it.
    x ∈ pre ++ post → (x ∈ pre ∧ live x) ∨ x ∈ post := by
  -- `x` is pending before the merge.
  intro hx
  -- The log is the prefix then the rest: `x` is in one of them.
  rcases List.mem_append.mp hx with hpre | hpost
  -- In the prefix, and live: the merge links it.
  · exact Or.inl ⟨hpre, hlive⟩
  -- In the rest: still pending.
  · exact Or.inr hpost

-- Promise kept by truncating only what was linked: a merge that links an empty prefix but truncates the whole log loses a live pending vector.
theorem truncate_through_head_loses :
    -- The log is one insert of vector 1; the merge links nothing and keeps nothing.
    let pre : Log := []
    -- What the bug leaves of the log.
    let kept : Log := []
    -- 1 is pending in the log [1], but neither linked nor kept.
    1 ∈ pre ++ [1] ∧ ¬ (1 ∈ pre ∨ 1 ∈ kept) := by
  -- Compute both sides: 1 is in [1], and nothing is in the empty list.
  simp

end PendingCache
