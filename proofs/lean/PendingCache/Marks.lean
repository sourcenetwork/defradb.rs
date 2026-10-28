import PendingCache.Latest

/-!
# PendingCache.Marks: skipping superseded entries without a map

The story, in plain words. A search must score each pending vector by the
newest entry it can see. Rather than build a map on every search, each
entry in the node's copy of the log carries a superseded marker: the
position of its vector's next entry. A search at head `h` skips an entry
whose marker is at or before `h`, and keeps every other entry it reaches.

The trap is the marker's own position. An update logged after a search's
snapshot sets the marker too, but that update is in the search's future;
skipping on it would leave the search with no entry for the vector at all.
So the marker counts only when it is in reach.

A tiny example. Vector 7 is logged at position 1, then updated at 2, so
entry 1's marker is 2. A search at head 2 skips entry 1 and keeps entry 2.
A search at head 1 keeps entry 1, since the marker is past its head; a
search that skipped on any marker would keep nothing.

What must never happen:
* a kept entry is not its vector's newest at or before the head
  (`kept_is_newest`);
* the newest entry at or before the head is skipped (`newest_is_kept`);
* two entries of one vector are both kept (`one_kept_per_vector`);
* the marker, which holds only the next entry's position, decides
  differently than looking at every later entry would (`next_decides`);
* and the law does not survive a marker read without the head, which is
  why the head is compared (`head_blind_marker_hides`).

Which markers exist when, under concurrent catch-ups and copies that lose
the publish, is the TLA+ model `proofs/tla/PendingCache.tla`
(`INV_ScoresLatest`, RED `MC_PendingCache_Red_HeadBlindMarks`).

Rust anchors: `Segment::push`, `Segment::supersede`, `Copy::caught_up` and
`Copy::for_each_latest_through`
(`crates/db/src/index/vector/engine/diskann/pending_cache.rs`).
-/


namespace PendingCache

-- Whether some later entry of `e`'s vector is in reach of a search at `head`: what a marker in reach says.
def superseded (log : List Entry) (head : Nat) (e : Entry) : Prop :=
  -- A later entry of the same vector, at or before the head.
  ∃ f ∈ log, f.id = e.id ∧ e.pos < f.pos ∧ f.pos ≤ head

-- What a search at `head` keeps of entry `e`: reached, and not superseded in reach.
def kept (log : List Entry) (head : Nat) (e : Entry) : Prop :=
  -- In reach, and no newer entry of its vector in reach.
  e.pos ≤ head ∧ ¬ superseded log head e

-- Promise: a kept entry is its vector's newest at or before the head.
theorem kept_is_newest (log : List Entry) (head : Nat) (e : Entry)
    (hkept : kept log head e) :
    -- Every entry of its vector in reach is no newer.
    ∀ f ∈ log, f.id = e.id → f.pos ≤ head → f.pos ≤ e.pos := by
  -- Take an entry `f` of the same vector in reach.
  intro f hf hid hreach
  -- Were `f` newer, `e` would be superseded in reach, which it is not.
  exact Nat.le_of_not_lt (fun hlt => hkept.2 ⟨f, hf, hid, hlt, hreach⟩)

-- Promise: the newest entry at or before the head is kept.
theorem newest_is_kept (log : List Entry) (head : Nat) (e : Entry)
    (hreach : e.pos ≤ head)
    (hnewest : ∀ f ∈ log, f.id = e.id → f.pos ≤ head → f.pos ≤ e.pos) :
    -- It is kept.
    kept log head e := by
  -- It is in reach; suppose some newer entry `f` in reach superseded it.
  refine ⟨hreach, fun ⟨f, hf, hid, hlt, hfreach⟩ => ?_⟩
  -- Then `f` is both newer than `e` and no newer: impossible.
  exact Nat.lt_irrefl _ (Nat.lt_of_lt_of_le hlt (hnewest f hf hid hfreach))

-- Promise: two kept entries of one vector sit at the same position, so with one entry per position the search keeps one per vector.
theorem one_kept_per_vector (log : List Entry) (head : Nat) (e f : Entry)
    (he : e ∈ log) (hf : f ∈ log) (hid : f.id = e.id)
    (hke : kept log head e) (hkf : kept log head f) :
    -- Their positions agree.
    e.pos = f.pos := by
  -- Each is no newer than the other.
  exact Nat.le_antisymm
    (kept_is_newest log head f hkf e he hid.symm hke.1)
    (kept_is_newest log head e hke f hf hid hkf.1)

-- Promise: when `next` is the position of the vector's next entry (a later entry, and no later entry comes before it), comparing `next` with the head decides superseded exactly.
theorem next_decides (log : List Entry) (head : Nat) (e : Entry) (next : Nat)
    -- `next` is the position of a later entry of the vector.
    (hnext : ∃ f ∈ log, f.id = e.id ∧ e.pos < f.pos ∧ f.pos = next)
    -- And every later entry of the vector is at `next` or after.
    (hfirst : ∀ f ∈ log, f.id = e.id → e.pos < f.pos → next ≤ f.pos) :
    -- The marker in reach means superseded in reach.
    next ≤ head ↔ superseded log head e := by
  constructor
  -- The marker in reach: its own entry is the later one in reach.
  · intro hle
    obtain ⟨f, hf, hid, hlt, hpos⟩ := hnext
    exact ⟨f, hf, hid, hlt, hpos ▸ hle⟩
  -- A later entry in reach: the next one is no later than it.
  · intro ⟨f, hf, hid, hlt, hreach⟩
    exact Nat.le_trans (hfirst f hf hid hlt) hreach

-- Promise kept by comparing the head: a search at head 1 whose vector 7 was updated at position 2 keeps its entry 1, but skipping on any marker would keep no entry of 7.
theorem head_blind_marker_hides :
    -- Vector 7 at position 1, updated at 2.
    let log : List Entry := [⟨1, 7, 1⟩, ⟨2, 7, 2⟩]
    -- At head 1, entry 1 is kept; a marker read without the head (head as large as the update) supersedes it.
    kept log 1 ⟨1, 7, 1⟩ ∧ superseded log 2 ⟨1, 7, 1⟩ := by
  -- Unfold both and check the two entries.
  simp [kept, superseded]

end PendingCache
