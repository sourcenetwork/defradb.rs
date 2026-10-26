/-!
# PendingCache.Latest: which code a search scores an updated vector by

The story, in plain words. Updating a pending vector appends a new entry
to the pending log, so a vector updated twice has two entries, each with
its own code. A search must rank the vector by the newest entry it can
see, never an older one: an old code puts the vector where it used to be,
and enough of those crowd the nearest vector out of the search's shortlist.

A search visits the entries oldest first and keeps, per vector, the code
of the last one it visited. Its own transaction's appends come after every
log entry, so the newest of them wins.

The node's in-memory copy of the log may be ahead of a search: it holds
entries logged after the search's snapshot. Each entry keeps its log
position, and the search visits only the entries at or before its head.

A tiny example. Vector 7 is logged with code 1 at position 1, then
updated with code 2 at position 2. A search at head 2 scores 7 by code 2.
A search at head 1, given a copy holding both, skips position 2 and
scores 7 by code 1; without the cut it would score 7 by code 2, a code
from its future.

What must never happen:
* the code a search keeps for a vector is not that of the vector's last
  entry it visited (`scored_is_last`);
* an append after everything else, such as the transaction's own, does
  not decide the vector's code (`scored_append`);
* a copy ahead of a search, cut at the search's head, scores anything
  differently than the search's own log would (`cut_scores_snapshot`);
* and the law does not survive leaving out the cut, which is why the copy
  keeps positions (`uncut_scores_future`).

Which entries a search may see, under concurrent commits, merges and
copies published by other searches, is the TLA+ model
`proofs/tla/PendingCache.tla` (`INV_ScoresLatest`); this file proves the
per-search laws it rests on, for logs of any length.

Rust anchors: `pending_candidates` and `snapshot_pending`
(`crates/db/src/index/vector/engine/diskann/graph.rs`), and
`Copy::for_each_through` and `Segment`
(`crates/db/src/index/vector/engine/diskann/pending_cache.rs`).
-/

namespace PendingCache

-- One log entry: the position it was logged at, the vector it inserts, and that vector's code.
structure Entry where
  -- Its log position; the transaction's own appends sit past every one.
  pos : Nat
  -- The vector it inserts.
  id : Nat
  -- The code it gives that vector.
  code : Nat

-- The code a search keeps for vector `x` after visiting `entries` oldest first: each visit of `x` overwrites the one before, as inserting into the map does.
def scored (entries : List Entry) (x : Nat) : Option Nat :=
  -- Start with no code; each entry of `x` replaces what is kept.
  entries.foldl (fun kept e => if e.id = x then some e.code else kept) none

-- Promise: visiting one more entry keeps its code when it is of `x`, and changes nothing otherwise.
theorem scored_append (entries : List Entry) (e : Entry) (x : Nat) :
    -- The kept code after the extra visit.
    scored (entries ++ [e]) x = if e.id = x then some e.code else scored entries x := by
  -- A fold over `entries ++ [e]` is the fold over `entries`, then one step for `e`.
  simp [scored, List.foldl_append]

-- Promise: the code kept for `x` is the code of `x`'s last entry visited, and none when `x` has no entry.
theorem scored_is_last (entries : List Entry) (x : Nat) :
    -- The kept code is that of the last entry of `x`.
    scored entries x = ((entries.filter (fun e => e.id = x)).getLast?).map Entry.code := by
  -- Build `entries` up from the empty list, one entry appended at a time: walk its reverse, whose head is the last entry appended.
  have walk : ∀ rev : List Entry, scored rev.reverse x
      = ((rev.reverse.filter (fun e => e.id = x)).getLast?).map Entry.code := by
    -- Take the reversed entries apart from the front.
    intro rev
    -- Induct on them.
    induction rev with
    -- No entries: nothing kept, and no last entry of `x`.
    | nil => rfl
    -- One more entry `e`, appended after `rest`, for which the law already holds.
    | cons e rest ih =>
      -- Split on whether `e` is an entry of `x`.
      by_cases h : e.id = x
      -- It is: the extra visit keeps `e`'s code, and `e` is `x`'s last entry.
      · simp [scored_append, h, List.filter_append]
      -- It is not: neither the kept code nor `x`'s last entry changes.
      · simp [scored_append, h, List.filter_append, ih]
  -- `entries` is the reverse of its reverse.
  simpa using walk entries.reverse

-- The entries of a copy a search visits: those at or before its head.
def cut (head : Nat) (entries : List Entry) : List Entry :=
  -- Keep the entries whose position the search's snapshot has reached.
  entries.filter (fun e => e.pos ≤ head)

-- Promise: a copy that is the search's own log followed by later entries, all past its head, scores every vector the way that log does once cut at the head.
theorem cut_scores_snapshot (head : Nat) (snap later : List Entry) (x : Nat)
    -- Every entry the search's snapshot has is at or before its head.
    (hsnap : ∀ e ∈ snap, e.pos ≤ head)
    -- Every entry the copy has beyond them is past it.
    (hlater : ∀ e ∈ later, head < e.pos) :
    -- Cut at the head, the copy scores `x` as the snapshot's log does.
    scored (cut head (snap ++ later)) x = scored snap x := by
  -- The cut keeps all of `snap`.
  have hkeep : snap.filter (fun e => e.pos ≤ head) = snap :=
    List.filter_eq_self.mpr (fun e he => by simpa using hsnap e he)
  -- The cut drops all of `later`.
  have hdrop : later.filter (fun e => e.pos ≤ head) = [] :=
    List.filter_eq_nil_iff.mpr (fun e he => by simpa using hlater e he)
  -- So the cut copy is exactly `snap`.
  simp [cut, List.filter_append, hkeep, hdrop]

-- Promise kept by the cut: a search at head 1 handed a copy that also holds an update logged at position 2 scores the vector by the update's code when nothing cuts it.
theorem uncut_scores_future :
    -- The search's log: vector 7 with code 1 at position 1.
    let snap : List Entry := [⟨1, 7, 1⟩]
    -- The copy also has 7's update, code 2 at position 2.
    let copy : List Entry := snap ++ [⟨2, 7, 2⟩]
    -- Uncut, the copy scores 7 by code 2; the search's log, by code 1.
    scored copy 7 = some 2 ∧ scored snap 7 = some 1 ∧ scored (cut 1 copy) 7 = some 1 := by
  -- Compute all three folds.
  decide

end PendingCache
