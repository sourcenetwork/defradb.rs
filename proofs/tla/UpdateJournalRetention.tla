---- MODULE UpdateJournalRetention ----
\* Writers add rows to one collection's update journal, each row taking the
\* next cursor. Truncates of the whole collection drop rows, a slice at a
\* time, and two of them can run at once. A reader keeps a cursor and reads
\* the rows after it, one page at a time.
\* What must never happen: the dropped rows stop being one run from cursor 1
\* (a hole in the middle), a reader's cursor moves past a row it was neither
\* handed nor told it lost, or a read finds a row missing below rows that are
\* kept. A reader whose next row was dropped is told so, with the oldest
\* cursor it can read after, and loses exactly the rows before that cursor.
\* Not modeled: which rows a reader may see (DocumentUpdateJournal.tla), and
\* the cursor-beyond-head refusal, which changes nothing.
\* Rust owners: crates/database/db/src/event/update_retention.rs
\* (drop_rows, oldest_row, missing_row), crates/database/db/src/event/updates.rs
\* (read, rows, scanned) and crates/database/db/src/collection/ops/truncate/all.rs
\* (truncate_documents).
EXTENDS Naturals, FiniteSets

\* MaxRows bounds how many rows are ever added; Slice is the most rows one
\* truncate transaction drops; Limit is the most rows one page reads;
\* Truncators are the truncates that may run; Mode picks the bug a RED plants.
CONSTANTS MaxRows, Slice, Limit, Truncators, Mode
\* At least one row is added.
ASSUME /\ MaxRows \in Nat \ {0}
\* A slice drops at least one row.
       /\ Slice \in Nat \ {0}
\* A page reads at least one row.
       /\ Limit \in Nat \ {0}
\* "safe" is the design; every other mode breaks exactly one guard.
       /\ Mode \in {"safe", "anywhere", "skip", "reset"}

\* No truncate target yet.
None == 0
\* The smaller of two numbers.
Min(x, y) == IF x < y THEN x ELSE y

\* added counts the rows ever added, and each row is known by that number;
\* head is the newest cursor; at maps a cursor to the row it holds; given maps
\* a row to the cursor it was given; dropped holds the cursors whose rows were
\* dropped; target maps each truncate to the head it read, None when it is not
\* running; ran holds the truncates that have started once.
\* cursor is the reader's checkpoint; got holds the rows it was handed; told
\* holds the rows it was told it lost; corrupt says a read found a row missing
\* below kept rows.
VARIABLES added, head, at, given, dropped, target, ran,
          cursor, got, told, corrupt
\* Every variable.
vars == <<added, head, at, given, dropped, target, ran, cursor, got, told, corrupt>>

\* The cursors whose rows are still kept.
Kept == (1..head) \ dropped
\* The oldest kept cursor, or head + 1 when none is kept.
OldestKept == IF Kept = {} THEN head + 1 ELSE CHOOSE c \in Kept : \A k \in Kept : c <= k
\* The cursor a reader can read after once its next row is gone: the one
\* just before the oldest kept row, which is head when nothing is kept.
Oldest == OldestKept - 1

\* A new journal is empty, nothing is truncating, and the reader is at 0.
Init == /\ added = 0
        /\ head = 0
        /\ at = [c \in 1..MaxRows |-> 0]
        /\ given = [r \in 1..MaxRows |-> 0]
        /\ dropped = {}
        /\ target = [t \in Truncators |-> None]
        /\ ran = {}
        /\ cursor = 0
        /\ got = {}
        /\ told = {}
        /\ corrupt = FALSE

\* A write commits a row: the next row number at the next cursor.
Add ==
  /\ added < MaxRows
  /\ head < MaxRows
  /\ added' = added + 1
  /\ head' = head + 1
  /\ at' = [at EXCEPT ![head + 1] = added + 1]
  /\ given' = [given EXCEPT ![added + 1] = head + 1]
  /\ UNCHANGED <<dropped, target, ran, cursor, got, told, corrupt>>

\* A truncate without a filter starts once its documents are deleted: it
\* reads the head, and will drop every row up to it.
Start(t) ==
  /\ t \notin ran
  /\ target' = [target EXCEPT ![t] = head]
  /\ ran' = ran \cup {t}
  /\ UNCHANGED <<added, head, at, given, dropped, cursor, got, told, corrupt>>

\* One truncate transaction drops up to Slice rows, starting at the oldest
\* kept row, and the truncate ends once nothing up to its target is kept.
\* The "anywhere" bug starts the slice at any kept row up to the target.
\* The "reset" bug ends the truncate by emptying the journal and setting the
\* head back to 0, so the next row takes cursor 1 again.
DropSlice(t) ==
  /\ target[t] # None
  /\ IF OldestKept > target[t]
       THEN /\ target' = [target EXCEPT ![t] = None]
            /\ IF Mode = "reset"
                 THEN /\ head' = 0
                      /\ dropped' = {}
                 ELSE UNCHANGED <<head, dropped>>
            /\ UNCHANGED <<added, at, given>>
       ELSE \E first \in {c \in Kept : c <= target[t]} :
              /\ Mode = "anywhere" \/ first = OldestKept
              /\ dropped' = dropped \cup (first..Min(target[t], first + Slice - 1))
              /\ UNCHANGED <<target, added, head, at, given>>
  /\ UNCHANGED <<ran, cursor, got, told, corrupt>>

\* The reader reads one page after its cursor, in one snapshot. Its next row
\* kept: it is handed the rows up to Limit and moves past them, unless one of
\* them is missing, which is corruption. Its next row dropped: when no row at
\* or before it is kept it is told it lost the rows up to Oldest and moves to
\* Oldest; otherwise a row is missing, which is corruption.
\* The "skip" bug answers a dropped next row with the rows that are left, as
\* if nothing was lost.
Read ==
  /\ cursor < head
  /\ LET next == Min(head, cursor + Limit)
         page == (cursor + 1)..next
     IN IF cursor + 1 \in Kept
          THEN IF page \subseteq Kept
                 THEN /\ got' = got \cup {at[c] : c \in page}
                      /\ cursor' = next
                      /\ UNCHANGED <<told, corrupt>>
                 ELSE /\ corrupt' = TRUE
                      /\ UNCHANGED <<got, told, cursor>>
          ELSE IF OldestKept > cursor + 1
                 THEN IF Mode = "skip"
                        THEN /\ got' = got \cup {at[c] : c \in (OldestKept..Min(head, OldestKept + Limit - 1))}
                             /\ cursor' = Min(head, OldestKept + Limit - 1)
                             /\ UNCHANGED <<told, corrupt>>
                        ELSE /\ told' = told \cup {at[c] : c \in (cursor + 1)..Oldest}
                             /\ cursor' = Oldest
                             /\ UNCHANGED <<got, corrupt>>
                 ELSE /\ corrupt' = TRUE
                      /\ UNCHANGED <<got, told, cursor>>
  /\ UNCHANGED <<added, head, at, given, dropped, target, ran>>

\* Each step adds a row, starts a truncate, drops a slice, or reads a page.
Next ==
  \/ Add
  \/ \E t \in Truncators : Start(t) \/ DropSlice(t)
  \/ Read
\* The system starts in Init and then only takes those steps.
Spec == Init /\ [][Next]_vars

\* The dropped rows are one run from cursor 1: no kept row sits below a
\* dropped one.
INV_Prefix == \A d \in dropped : \A k \in Kept : d < k
\* No read ever finds a row missing below rows that are kept.
INV_NoCorrupt == ~corrupt
\* Every row given a cursor at or before the reader's was handed to it or
\* reported lost to it: the reader never moves past a row silently.
INV_NoSilentGap ==
  \A r \in 1..added : given[r] <= cursor => r \in got \cup told
\* The reader is told it lost a row only when that row was dropped.
INV_ToldOnlyDropped == \A r \in told : given[r] \in dropped
====
