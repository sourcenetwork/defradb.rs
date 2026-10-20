/-!
# PendingCache.Covers: a loaded cache plus logged inserts covers every snapshot

The story, in plain words. A DiskANN index keeps the vectors it has not
linked into its graph yet in a pending list, and keeps a copy of that list in
memory. The copy is loaded once from a scan of the list, at some log head.
After that, every insert and every delete of a pending vector appends one
entry to a commit-ordered log, and a search catches the copy up by adding
the vector of every insert entry it has not seen. Delete entries add
nothing: the copy only grows.

A search whose snapshot sits anywhere between the load and the newest entry
the copy has seen must find every vector its snapshot has pending in the
copy. Its pending list is the loaded list with some prefix of the log's
entries applied, so the promise is a law about lists of entries, for any
number of them.

A tiny example. The copy is loaded with nothing pending. The log then holds
"insert 1", "delete 1". The copy becomes {1}. A search at the snapshot
after the insert has 1 pending and finds it. A search at the snapshot after
the delete has nothing pending; the extra 1 costs it only a dropped
candidate.

What must never happen:
* a snapshot in the window has a vector pending that the copy lacks
  (`window_covered`);
* catching up in two steps gives a different copy than catching up in one
  (`catch_up_in_steps`);
* and the law does not survive keeping only what is still pending at the
  catch-up's own snapshot, which is why an insert entry carries its code
  (`still_pending_filter_loses`).

The order of commits, the generation tag, the load check and a
transaction's own writes are the TLA+ model `proofs/tla/PendingCache.tla`;
this file proves the covering law it rests on for logs of any length.

Rust anchors: `PendingCache` and its catch-up
(`crates/db/src/index/vector/engine/diskann/pending_cache.rs`), the appends in
`insert` and `delete` (`crates/db/src/index/vector/engine/diskann/mod.rs`).
-/

namespace PendingCache

-- One log entry: a vector put on the pending list, or taken off it.
inductive Entry where
  -- An insert or an update of vector `n`.
  | ins (n : Nat)
  -- A delete of vector `n`.
  | del (n : Nat)

-- A pending list, as the set of vectors on it: `s x` says vector `x` is pending.
abbrev Pending := Nat → Prop

-- What one entry does to a pending list.
def step (s : Pending) : Entry → Pending
  -- An insert puts its vector on the list, and leaves the rest as it was.
  | .ins n => fun x => x = n ∨ s x
  -- A delete takes its vector off, and leaves the rest as it was.
  | .del n => fun x => x ≠ n ∧ s x

-- The pending list after a run of entries, applied oldest first.
def run (s : Pending) (log : List Entry) : Pending :=
  -- Apply each entry in turn to the list the earlier ones left.
  log.foldl step s

-- The vectors a run of entries inserts: what a catch-up adds to the copy.
def inserted (log : List Entry) : Pending :=
  -- Vector `x` is added when some entry of the run inserts it.
  fun x => Entry.ins x ∈ log

-- The copy after a load of `s` and a catch-up over `log`: the loaded vectors and every one the log inserts.
def copy (s : Pending) (log : List Entry) : Pending :=
  -- Loaded, or inserted since.
  fun x => s x ∨ inserted log x

-- One entry: whatever is pending after it was pending before it, or the entry inserted it.
theorem step_sub (s : Pending) (e : Entry) (x : Nat) :
    -- After `e`, `x` pending means: pending before, or `e` inserts `x`.
    step s e x → s x ∨ e = .ins x := by
  -- Look at which kind of entry `e` is.
  cases e with
  -- An insert of `n`:
  | ins n =>
    -- `x` is pending after it: `x` is `n`, or it was pending before.
    intro h
    -- Split on those two cases.
    rcases h with h | h
    -- `x` is `n`: the entry inserts `x`.
    · exact Or.inr (by rw [h])
    -- `x` was pending before: that is the first case.
    · exact Or.inl h
  -- A delete of `n`: `x` pending after it was pending before it.
  | del n => exact fun h => Or.inl h.2

-- A run: whatever is pending after it was pending before it, or the run inserted it.
theorem run_sub (s : Pending) (log : List Entry) (x : Nat) :
    -- After `log`, `x` pending means: pending before, or `log` inserts `x`.
    run s log x → s x ∨ inserted log x := by
  -- Induction on the run, for every starting list at once.
  induction log generalizing s with
  -- No entries: the list is unchanged, so `x` was pending before.
  | nil => exact fun h => Or.inl h
  -- A first entry `e`, then the rest `rest`:
  | cons e rest ih =>
    -- `x` is pending after the whole run.
    intro h
    -- The rest ran from the list `e` left: `x` was pending after `e`, or `rest` inserts it.
    rcases ih (step s e) h with h | h
    -- Pending after `e`: before `e`, or `e` inserts it.
    · rcases step_sub s e x h with h | h
      -- Pending before the run.
      · exact Or.inl h
      -- `e` inserts `x`, and `e` is the run's first entry.
      · exact Or.inr (by rw [inserted, h]; exact List.mem_cons_self _ _)
    -- `rest` inserts `x`, and `rest` is part of the run.
    · exact Or.inr (List.mem_cons_of_mem e h)

-- Promise: a copy loaded at `s` and caught up over `seen ++ later` holds every vector pending at any snapshot in between, the one after `seen`.
theorem window_covered (s : Pending) (seen later : List Entry) (x : Nat) :
    -- Pending at the snapshot after `seen` means: in the copy.
    run s seen x → copy s (seen ++ later) x := by
  -- `x` is pending at that snapshot.
  intro h
  -- It was loaded, or `seen` inserted it.
  rcases run_sub s seen x h with h | h
  -- Loaded: the copy holds it.
  · exact Or.inl h
  -- Inserted by `seen`, which the copy's log starts with.
  · exact Or.inr (List.mem_append_left later h)

-- Promise: catching up over `a`, then over `b`, gives the copy that catching up over `a ++ b` at once gives.
theorem catch_up_in_steps (s : Pending) (a b : List Entry) (x : Nat) :
    -- The copy after two catch-ups is the copy after one.
    copy (copy s a) b x ↔ copy s (a ++ b) x := by
  -- Unfold the copies into "loaded, or inserted by a, or inserted by b".
  simp only [copy, inserted, List.mem_append]
  -- Both sides say the same thing, grouped differently.
  exact or_assoc

-- Promise kept by not filtering: a catch-up that keeps only what is still pending at its own snapshot loses a vector an earlier snapshot in its window has pending.
theorem still_pending_filter_loses :
    -- Nothing loaded; the log inserts vector 1 and then deletes it.
    let s : Pending := fun _ => False
    -- The log the catch-up reads.
    let log := [Entry.ins 1, Entry.del 1]
    -- At the snapshot after the insert, 1 is pending, and the filtered copy lacks it.
    run s [Entry.ins 1] 1 ∧ ¬ (s 1 ∨ (inserted log 1 ∧ run s log 1)) := by
  -- Compute both sides: 1 is pending after the insert, and gone after the delete.
  simp [run, step, inserted]

end PendingCache
