/-!
The update journal of one collection numbers its rows 1, 2, 3, ... and a
truncate of the whole collection drops rows from the oldest one still kept.
So the dropped rows are always one run from row 1 up to some row `t`, and the
journal is described by two numbers: `t`, the last dropped row (0 for none),
and `head`, the newest row. Every row from `t + 1` to `head` is kept.

A reader holds a cursor `after` and asks for the rows after it. It is refused
when the cursor is past the head, told the cursor is no longer available when
the row after it was dropped, and otherwise handed a run of kept rows.

What is proved here: a slice that starts at the oldest kept row keeps the
dropped rows one run; a read that is answered with rows is handed only kept
rows and only when nothing after its cursor was dropped; a cursor is refused
as no longer available exactly when a row after it was dropped; the cursor the
refusal names is one a read can start from; and the Rust test for "dropped,
not missing" (the oldest kept row is past the next cursor) means the same
thing as "the next row was dropped".

Rust owners: crates/database/db/src/event/update_retention.rs (drop_rows,
oldest_row, missing_row) and crates/database/db/src/event/updates.rs (read,
rows, scanned).
-/

namespace UpdateJournalRetention

-- A journal: rows 1 to `dropped` are gone, rows `dropped + 1` to `head` are
-- kept, and `dropped` never passes `head`.
structure Journal where
  dropped : Nat
  head : Nat
  bound : dropped ≤ head

-- Row `p` is dropped when it is one of rows 1 to `dropped`.
def isDropped (j : Journal) (p : Nat) : Prop := 1 ≤ p ∧ p ≤ j.dropped

-- Row `p` is kept when it lies after the dropped run and at or before the head.
def isKept (j : Journal) (p : Nat) : Prop := j.dropped < p ∧ p ≤ j.head

-- What a read answers: refused past the head, no longer available with the
-- cursor to read after, or the rows `first` to `last` (none when first > last).
inductive Answer where
  | beyondHead
  | unavailable (oldest : Nat)
  | rows (first last : Nat)
  deriving DecidableEq, Repr

-- The read the Rust code makes: a cursor past the head is refused; a cursor
-- whose next row is dropped is told to read after the last dropped row; any
-- other cursor gets the rows after it, up to `limit` of them.
def read (j : Journal) (after limit : Nat) : Answer :=
  if j.head < after then Answer.beyondHead
  else if after < j.dropped then Answer.unavailable j.dropped
  else Answer.rows (after + 1) (min j.head (after + limit))

-- One truncate slice from the oldest kept row `dropped + 1` up to `last`
-- (no further than the head): the rows it drops join the dropped run.
def dropSlice (j : Journal) (last : Nat) (h : last ≤ j.head) : Journal :=
  { dropped := max j.dropped last, head := j.head, bound := by have := j.bound; omega }

-- A slice starting at the oldest kept row leaves the dropped rows one run:
-- a row is dropped afterwards exactly when it was dropped before or the
-- slice covered it.
theorem slice_keeps_one_run (j : Journal) (last : Nat) (h : last ≤ j.head) (p : Nat) :
    isDropped (dropSlice j last h) p ↔
      (isDropped j p ∨ (j.dropped + 1 ≤ p ∧ p ≤ last ∧ 1 ≤ p)) := by
  simp only [isDropped, dropSlice]
  omega

-- A slice never moves the head, so no cursor is ever handed out twice.
theorem slice_keeps_head (j : Journal) (last : Nat) (h : last ≤ j.head) :
    (dropSlice j last h).head = j.head := rfl

-- Every row a read hands over is kept.
theorem rows_are_kept (j : Journal) (after limit first last p : Nat)
    (hr : read j after limit = Answer.rows first last) (hp : first ≤ p ∧ p ≤ last) :
    isKept j p := by
  unfold read at hr
  split at hr
  · cases hr
  · split at hr
    · cases hr
    · cases hr
      simp only [isKept]
      omega

-- A read answered with rows starts right after its cursor and no row after
-- the cursor was dropped: the reader skips nothing silently.
theorem rows_skip_nothing (j : Journal) (after limit first last : Nat)
    (hr : read j after limit = Answer.rows first last) :
    first = after + 1 ∧ ∀ p, after < p → ¬ isDropped j p := by
  unfold read at hr
  split at hr
  · cases hr
  · split at hr
    · cases hr
    · cases hr
      refine ⟨rfl, ?_⟩
      intro p hp
      simp only [isDropped]
      omega

-- A cursor at or before the head is told it is no longer available exactly
-- when the row after it was dropped.
theorem unavailable_iff_next_dropped (j : Journal) (after limit : Nat)
    (hh : after ≤ j.head) :
    (∃ oldest, read j after limit = Answer.unavailable oldest) ↔ isDropped j (after + 1) := by
  simp only [read, isDropped]
  constructor
  · intro ⟨oldest, h⟩
    split at h
    · omega
    · split at h
      · omega
      · cases h
  · intro h
    refine ⟨j.dropped, ?_⟩
    rw [if_neg (by omega), if_pos (by omega)]

-- The cursor a refusal names is one a read can start from: reading after it
-- is answered with rows.
theorem oldest_is_readable (j : Journal) (after limit oldest : Nat)
    (hr : read j after limit = Answer.unavailable oldest) :
    ∃ first last, read j oldest limit = Answer.rows first last := by
  unfold read at hr
  split at hr
  · cases hr
  · split at hr
    · cases hr
      refine ⟨j.dropped + 1, min j.head (j.dropped + limit), ?_⟩
      have := j.bound
      simp only [read]
      rw [if_neg (by omega), if_neg (by omega)]
    · cases hr

-- The refusal loses only dropped rows: every row between the cursor and the
-- cursor it names was dropped.
theorem unavailable_loses_only_dropped (j : Journal) (after limit oldest : Nat)
    (hr : read j after limit = Answer.unavailable oldest) :
    ∀ p, after < p → p ≤ oldest → isDropped j p := by
  unfold read at hr
  split at hr
  · cases hr
  · split at hr
    · cases hr
      intro p hp hq
      simp only [isDropped]
      omega
    · cases hr

-- The oldest kept row, which the Rust code finds by seeking to the first row
-- key: `dropped + 1`, or `head + 1` when nothing is kept.
def oldestKept (j : Journal) : Nat := j.dropped + 1

-- The Rust test for "the row after the cursor was dropped, not lost": the
-- oldest kept row is past the next cursor. On a journal whose dropped rows
-- are one run, that is the same as the next row being dropped, and the cursor
-- the Rust code names (the oldest kept row less one) is the last dropped row.
theorem seek_test_matches (j : Journal) (after : Nat) :
    (after + 1 < oldestKept j ↔ isDropped j (after + 1)) ∧ oldestKept j - 1 = j.dropped := by
  simp only [oldestKept, isDropped]
  omega

end UpdateJournalRetention
