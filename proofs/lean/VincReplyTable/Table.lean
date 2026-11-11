/-!
# VincReplyTable.Table: the reply table hands each stream out once, under an id never used before

The story, in plain words. A vinc endpoint keeps the send stream of every
request still owed a reply in a table, under an id, and gives the engine a
token carrying only that id. The table can take an entry out three ways: by
its id (a reply or a dropped token), every entry of one connection (the
connection ended), or every entry past its deadline (the sweep). It holds at
most `cap` entries; a request beyond that is refused.

What is promised:
- a full table refuses a new entry, and a table within its bound stays within
  it (`full_refuses`, `issue_bounded`);
- a new entry's id is one no entry has, so a late reply for an old id never
  reaches a new stream (`issue_fresh`), and that stays true after every
  operation (`issue_keeps_fresh`, `take_keeps_fresh`, `purge_keeps_fresh`,
  `sweep_keeps_fresh`);
- taking an id twice finds it the second time no more (`take_once`), and
  takes nothing else (`take_only`);
- a purge or a sweep removes exactly the entries it names and keeps the rest
  (`purge_only`, `sweep_only`).

An entry is its id, its connection and its deadline. The stream itself is left
out: the table never looks inside it.

Temporal behaviour (the token's drop, the lost message, the sweep's timing) is
the TLA+ model `proofs/tla/VincReplyBridge.tla`; this file carries the table
laws that model takes on trust.

Rust anchors: `ReplyTable::issue`, `ReplyTable::take`, `ReplyTable::purge` and
`ReplyTable::sweep` (`crates/p2p/src/vinc/reply_table.rs`).
-/

namespace VincReplyTable

-- One held entry: the id its token carries, its connection, and its deadline.
structure Entry where
  -- the id the engine's token names;
  id : Nat
  -- the connection the stream belongs to;
  conn : Nat
  -- the time after which the sweep resets it.
  due : Nat

-- The table: its entries, the next id to hand out, and how many it may hold.
structure Table where
  -- the entries now held;
  entries : List Entry
  -- the id the next entry gets;
  next : Nat
  -- the most entries held at once.
  cap : Nat

-- Hold a new entry for `conn`, due at `due`: its id and the new table, or
-- nothing when the table is full.
def issue (t : Table) (conn due : Nat) : Option (Nat × Table) :=
  -- With room, the entry gets the next id and the next id moves on;
  if t.entries.length < t.cap then
    some (t.next, { t with entries := ⟨t.next, conn, due⟩ :: t.entries, next := t.next + 1 })
  -- without room, nothing is held.
  else none

-- Take the entry with `id`: whether it was there, and the table without it.
def take (t : Table) (id : Nat) : Bool × Table :=
  -- It was there if some entry has the id; afterwards no entry has it.
  (t.entries.any (fun e => e.id == id), { t with entries := t.entries.filter (fun e => e.id != id) })

-- Drop every entry of connection `c`.
def purge (t : Table) (c : Nat) : Table :=
  -- Keep only the entries of other connections.
  { t with entries := t.entries.filter (fun e => e.conn != c) }

-- Drop every entry due at or before `now`.
def sweep (t : Table) (now : Nat) : Table :=
  -- Keep only the entries whose deadline is still ahead.
  { t with entries := t.entries.filter (fun e => !(decide (e.due ≤ now))) }

-- Every entry's id is below the next id: the ids handed out so far.
def Fresh (t : Table) : Prop :=
  -- No entry has an id the table has not handed out yet.
  ∀ e ∈ t.entries, e.id < t.next

-- Promise: a table at its bound refuses a new entry.
theorem full_refuses (t : Table) (conn due : Nat)
    -- Given that the table already holds `cap` entries or more,
    (h : t.cap ≤ t.entries.length) :
    -- issuing holds nothing.
    issue t conn due = none := by
  -- Unfold issue; the room test fails.
  unfold issue
  -- The `if` takes its `else`.
  simp [Nat.not_lt.mpr h]

-- Promise: issuing into a table within its bound leaves it within its bound.
theorem issue_bounded (t t' : Table) (i conn due : Nat)
    -- Given that issuing succeeded with id `i` and table `t'`,
    (h : issue t conn due = some (i, t')) :
    -- the new table holds at most `cap` entries.
    t'.entries.length ≤ t'.cap := by
  -- Unfold issue and split on whether there was room.
  unfold issue at h
  split at h
  -- There was room: the new table has one more entry than a table below cap.
  · rename_i room
    -- Read off the new table from the success.
    cases h
    -- One more than a number below cap is at most cap.
    simp
    omega
  -- There was no room: issuing returned nothing, not a table.
  · cases h

-- Promise: a new entry's id is one no entry of a fresh table has.
theorem issue_fresh (t t' : Table) (i conn due : Nat)
    -- Given a fresh table,
    (hf : Fresh t)
    -- and that issuing succeeded with id `i`,
    (h : issue t conn due = some (i, t')) :
    -- no entry of the table before had id `i`.
    ∀ e ∈ t.entries, e.id ≠ i := by
  -- Unfold issue and split on the room test.
  unfold issue at h
  split at h
  -- There was room: `i` is the next id.
  · cases h
    -- Each entry's id is below the next id, so it is not the next id.
    intro e he
    have := hf e he
    omega
  -- No room: issuing returned nothing.
  · cases h

-- Promise: issuing keeps the table fresh.
theorem issue_keeps_fresh (t t' : Table) (i conn due : Nat)
    -- Given a fresh table,
    (hf : Fresh t)
    -- and a successful issue,
    (h : issue t conn due = some (i, t')) :
    -- the new table is fresh.
    Fresh t' := by
  -- Unfold issue and split on the room test.
  unfold issue at h
  split at h
  -- There was room: the new entry has the old next id, below the new next id.
  · cases h
    intro e he
    -- The entry is the new one or an old one.
    simp at he
    rcases he with he | he
    -- The new one: its id is the old next id.
    · subst he
      simp
    -- An old one: its id was below the old next id, so below the new one.
    · have := hf e he
      simp
      omega
  -- No room: issuing returned nothing.
  · cases h

-- Promise: taking an id twice finds it the second time no more.
theorem take_once (t : Table) (id : Nat) :
    -- The second take of the same id reports it absent.
    (take (take t id).2 id).1 = false := by
  -- Unfold take: the second looks in the entries the first kept.
  simp [take, List.any_filter]

-- Promise: taking an id removes exactly the entries with that id.
theorem take_only (t : Table) (id : Nat) (e : Entry) :
    -- An entry is left after the take exactly when it was there and has another id.
    e ∈ (take t id).2.entries ↔ e ∈ t.entries ∧ e.id ≠ id := by
  -- A filter keeps what passes its test.
  simp [take, List.mem_filter]

-- Promise: a purge removes exactly the entries of its connection.
theorem purge_only (t : Table) (c : Nat) (e : Entry) :
    -- An entry is left after the purge exactly when it was there and is of another connection.
    e ∈ (purge t c).entries ↔ e ∈ t.entries ∧ e.conn ≠ c := by
  -- A filter keeps what passes its test.
  simp [purge, List.mem_filter]

-- Promise: a sweep removes exactly the entries already due.
theorem sweep_only (t : Table) (now : Nat) (e : Entry) :
    -- An entry is left after the sweep exactly when it was there and is not due yet.
    e ∈ (sweep t now).entries ↔ e ∈ t.entries ∧ now < e.due := by
  -- A filter keeps what passes its test; "not due" is "due later".
  simp [sweep, List.mem_filter, Nat.not_le]

-- Promise: taking keeps the table fresh.
theorem take_keeps_fresh (t : Table) (id : Nat) (hf : Fresh t) :
    -- Every entry left still has an id below the next id.
    Fresh (take t id).2 := by
  -- An entry left was an entry before, and the next id did not change.
  intro e he
  exact hf e ((take_only t id e).mp he).1

-- Promise: a purge keeps the table fresh.
theorem purge_keeps_fresh (t : Table) (c : Nat) (hf : Fresh t) :
    -- Every entry left still has an id below the next id.
    Fresh (purge t c) := by
  -- An entry left was an entry before, and the next id did not change.
  intro e he
  exact hf e ((purge_only t c e).mp he).1

-- Promise: a sweep keeps the table fresh.
theorem sweep_keeps_fresh (t : Table) (now : Nat) (hf : Fresh t) :
    -- Every entry left still has an id below the next id.
    Fresh (sweep t now) := by
  -- An entry left was an entry before, and the next id did not change.
  intro e he
  exact hf e ((sweep_only t now e).mp he).1

end VincReplyTable
