---- MODULE UpdateJournalRetention ----
\* Writers add rows to one collection's update journal, each row taking the
\* next cursor. A row is one block written to one document, and its once key
\* lets that block of that document add one row only, so a block merged twice
\* adds one row. Truncates of the whole collection delete the document and
\* drop rows, a slice at a time, and two of them can run at once; dropping a
\* row keeps its once key, which only regolith writes. The document can then
\* be created again, with the same content and the same blocks. A reader
\* keeps a cursor and reads the rows after it, one page at a time.
\* What must never happen: the dropped rows stop being one run from cursor 1
\* (a hole in the middle), a reader's cursor moves past a row it was neither
\* handed nor told it lost, a read finds a row missing below rows that are
\* kept, or a write commits and adds no row although no row was ever added
\* for it. A reader whose next row was dropped is told so, with the oldest
\* cursor it can read after, and loses exactly the rows before that cursor.
\* Not modeled: which rows a reader may see (DocumentUpdateJournal.tla), the
\* cursor-beyond-head refusal, which changes nothing, and a document created
\* while a truncate runs, before it reads the head: its rows are dropped and
\* a reader holding a cursor before them is told so, like any dropped row.
\* Rust owners: crates/database/db/src/event/update_retention.rs
\* (drop_rows, oldest_row, missing_row), crates/database/db/src/event/updates.rs
\* (read, rows, scanned) and crates/database/db/src/collection/ops/truncate/all.rs
\* (truncate_documents); the once key is updates.rs once_key, named by the
\* document's short ID, which a re-created document takes anew
\* (crates/database/db/src/docid/map.rs, truncate/all.rs truncate_chunk_in).
EXTENDS Naturals, FiniteSets

\* MaxRows bounds how many rows are ever added; Slice is the most rows one
\* truncate transaction drops; Limit is the most rows one page reads;
\* Truncators are the truncates that may run; Blocks are the blocks a write
\* can commit; MaxLives bounds how many times the document is created;
\* Mode picks the bug a RED plants.
CONSTANTS MaxRows, Slice, Limit, Truncators, Blocks, MaxLives, Mode
\* At least one row is added.
ASSUME /\ MaxRows \in Nat \ {0}
\* A slice drops at least one row.
       /\ Slice \in Nat \ {0}
\* A page reads at least one row.
       /\ Limit \in Nat \ {0}
\* The document is created at least once.
       /\ MaxLives \in Nat \ {0}
\* "safe" is the design; every other mode breaks exactly one guard.
       /\ Mode \in {"safe", "anywhere", "skip", "reset", "docid"}

\* No truncate target yet.
None == 0
\* The smaller of two numbers.
Min(x, y) == IF x < y THEN x ELSE y
\* A write is the document's life (which creation of it) and a block.
Writes == (1..MaxLives) \X Blocks
\* The once key of a write: its life and block, the way the short ID names
\* the life. The "docid" bug names only the block, the way the DocID would:
\* the same for every life of a document with the same content.
Key(w) == IF Mode = "docid" THEN w[2] ELSE w

\* added counts the rows ever added, and each row is known by that number;
\* head is the newest cursor; at maps a cursor to the row it holds; given maps
\* a row to the cursor it was given; dropped holds the cursors whose rows were
\* dropped; target maps each truncate to the head it read, None when it is not
\* running; ran holds the truncates that have started once.
\* wrote maps a row to the write that added it; once holds the once keys
\* that are held; written holds every write that committed; life is the
\* document's current life, and live says whether it exists now.
\* cursor is the reader's checkpoint; got holds the rows it was handed; told
\* holds the rows it was told it lost; corrupt says a read found a row missing
\* below kept rows.
VARIABLES added, head, at, given, dropped, target, ran,
          wrote, once, written, life, live,
          cursor, got, told, corrupt
\* Every variable.
vars == <<added, head, at, given, dropped, target, ran,
          wrote, once, written, life, live, cursor, got, told, corrupt>>

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
        /\ wrote = [r \in 1..MaxRows |-> <<0, 0>>]
        /\ once = {}
        /\ written = {}
        /\ life = 1
        /\ live = TRUE
        /\ cursor = 0
        /\ got = {}
        /\ told = {}
        /\ corrupt = FALSE

\* A write commits block b to the live document. When its once key is held
\* it adds nothing (the same block merged again); otherwise it adds the next
\* row number at the next cursor and holds its once key.
Add(b) ==
  /\ live
  /\ added < MaxRows
  /\ head < MaxRows
  /\ LET w == <<life, b>>
     IN /\ written' = written \cup {w}
        /\ IF Key(w) \in once
             THEN UNCHANGED <<added, head, at, given, wrote, once>>
             ELSE /\ added' = added + 1
                  /\ head' = head + 1
                  /\ at' = [at EXCEPT ![head + 1] = added + 1]
                  /\ given' = [given EXCEPT ![added + 1] = head + 1]
                  /\ wrote' = [wrote EXCEPT ![added + 1] = w]
                  /\ once' = once \cup {Key(w)}
  /\ UNCHANGED <<dropped, target, ran, life, live, cursor, got, told, corrupt>>

\* The document is created again after a truncate deleted it, with the same
\* content, so its blocks can be the ones it had: it starts a new life.
Recreate ==
  /\ ~live
  /\ life < MaxLives
  /\ life' = life + 1
  /\ live' = TRUE
  /\ UNCHANGED <<added, head, at, given, dropped, target, ran,
                 wrote, once, written, cursor, got, told, corrupt>>

\* A truncate without a filter deletes the document, then reads the head,
\* and will drop every row up to it.
Start(t) ==
  /\ t \notin ran
  /\ target' = [target EXCEPT ![t] = head]
  /\ ran' = ran \cup {t}
  /\ live' = FALSE
  /\ UNCHANGED <<added, head, at, given, dropped, wrote, once, written, life,
                 cursor, got, told, corrupt>>

\* One truncate transaction drops up to Slice rows, starting at the oldest
\* kept row, and the truncate ends once nothing up to its target is kept.
\* The "anywhere" bug starts the slice at any kept row up to the target.
\* The "reset" bug ends the truncate by emptying the journal and setting the
\* head back to 0, so the next row takes cursor 1 again. No slice touches a
\* once key: dropping a row keeps it.
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
  /\ UNCHANGED <<ran, wrote, once, written, life, live, cursor, got, told, corrupt>>

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
  /\ UNCHANGED <<added, head, at, given, dropped, target, ran,
                 wrote, once, written, life, live>>

\* Each step writes a block, creates the document again, starts a truncate,
\* drops a slice, or reads a page.
Next ==
  \/ \E b \in Blocks : Add(b)
  \/ Recreate
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
\* Every write that committed added a row, or repeats one that did: a write
\* never adds nothing because a dropped row of an earlier life held its key.
INV_EveryWriteHasRow == \A w \in written : \E r \in 1..added : wrote[r] = w
====
