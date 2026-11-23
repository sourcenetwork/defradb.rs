--------------------------- MODULE MaterializeResume ---------------------------
\* The story, in plain words.
\*
\* A collection has a few documents stored at an older version. A reindex
\* walks them a page at a time, saving its cursor with each page, and its
\* record says how much each page does to a document it walks:
\*   level 0: fix the index only (a schema change's settlement);
\*   level 1: also write the transformed value back (a rebuild asked for
\*            directly, such as reindexing with migrations);
\*   level 2: also move the version of a document whose path runs no
\*            transform (a materialization).
\* A crash (or an error) can stop a walk after any page, leaving the record
\* and its cursor behind. Whoever settles the owed reindex next (at open, after
\* the next schema change) carries on from that cursor, the way the record
\* says, except that a settlement never moves a version of its own accord, so
\* it does at most level 1.
\*
\* Then someone asks for a rebuild of level 1 or 2. It reuses the reindex
\* pages, so it could pick up the cursor left behind and carry on from there.
\*
\* What must never happen: a rebuild reports it is done while a document it
\* promised to handle was walked at a lower level than it asked for, because
\* it resumed past documents an earlier walk handled more lightly.
\*
\* How the code keeps the promise, three guards:
\*   1. a materialization (level 2) never resumes: it records a reindex of
\*      its own under a new token, and a cursor under another token reads as
\*      the start;
\*   2. a rebuild that writes values back resumes only a record that writes
\*      values back too;
\*   3. a page that resumes a record walks the way the record says, not the
\*      way its own caller would, so a settlement that carries on a rebuild's
\*      walk writes values back too.
\*
\* Rust code modelled:
\*   crates/database/db/src/definition/migration/reindex/mod.rs
\*     DB::reindex_in_pages (the owed reindex is resumed only by a rebuild
\*     that does not restamp and does not write back more than the record;
\*     otherwise a new token is owed)
\*   crates/database/db/src/definition/migration/reindex/owed.rs
\*     OwedReindex::persists_migrations (the record's level)
\*   crates/database/db/src/definition/migration/reindex/page.rs
\*     ReindexPage::step (a resumed page takes its mode from the record) and
\*     ReindexPage::walk (writes back only when the mode says so)
\*   crates/database/db/src/definition/migration/reindex/progress.rs read
\*     (a cursor under another token reads as the start)
EXTENDS Naturals

CONSTANTS
  \* How many documents the collection holds.
  N,
  \* TRUE when a materialization may resume a record (breaks guard 1).
  ResumeRestamp,
  \* TRUE when a rebuild resumes a record of a lower level (breaks guard 2).
  ResumeAnyMode,
  \* TRUE when a settlement walks at level 0 whatever the record says
  \* (breaks guard 3).
  SettleIgnoresMode

\* The constants have the shapes they should.
ASSUME /\ N \in Nat /\ N > 0
       /\ ResumeRestamp \in BOOLEAN
       /\ ResumeAnyMode \in BOOLEAN
       /\ SettleIgnoresMode \in BOOLEAN

\* The documents, by short ID in walk order.
Docs == 1..N

\* The levels a walk can apply.
Levels == 0..2

\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b

\* The larger of two numbers.
Max(a, b) == IF a > b THEN a ELSE b

VARIABLES
  \* For each document, the highest level any walk has applied to it.
  level,
  \* The cursor: the last document the owed reindex's walk passed.
  cursor,
  \* Whether a reindex is owed (a record stands).
  owed,
  \* The level the owed record says its pages walk at.
  recLevel,
  \* The rebuild's progress: "idle" (not running, or stopped by a crash),
  \* "walking", or "done" (it reported success).
  phase,
  \* The level the rebuild asked for.
  want

\* All the variables, for stuttering.
vars == <<level, cursor, owed, recLevel, phase, want>>

\* Every variable holds a value of its kind.
TypeOK == /\ level \in [Docs -> Levels]
          /\ cursor \in 0..N
          /\ owed \in BOOLEAN
          /\ recLevel \in Levels
          /\ phase \in {"idle", "walking", "done"}
          /\ want \in 1..2

\* At the start a schema change owes a settlement (level 0) that has walked
\* nothing, no walk has touched any document, and no rebuild is running.
Init == /\ level = [d \in Docs |-> 0]
        /\ cursor = 0
        /\ owed = TRUE
        /\ recLevel = 0
        /\ phase = "idle"
        /\ want = 1

\* Document d is walked at level l: it keeps the higher of what it had and l.
Apply(d, l) == level' = [level EXCEPT ![d] = Max(@, l)]

\* The level a settlement walks a page at: the record's, capped at 1 since a
\* settlement never restamps; level 0 with guard 3 broken.
SettleLevel == IF SettleIgnoresMode THEN 0 ELSE Min(recLevel, 1)

\* A settlement page, while no rebuild is walking: it walks the next document
\* at the settlement's level.
SettlePage == /\ phase # "walking"
              /\ owed
              /\ cursor < N
              /\ cursor' = cursor + 1
              /\ Apply(cursor + 1, SettleLevel)
              /\ UNCHANGED <<owed, recLevel, phase, want>>

\* The settlement's last page: past the last document, it clears the record.
SettleDone == /\ phase # "walking"
              /\ owed
              /\ cursor = N
              /\ owed' = FALSE
              /\ cursor' = 0
              /\ UNCHANGED <<level, recLevel, phase, want>>

\* Whether a rebuild of level w carries on the owed record: never for a
\* materialization (guard 1), and only from a record of at least its level
\* (guard 2).
Resumes(w) == /\ owed
              /\ (w < 2 \/ ResumeRestamp)
              /\ (recLevel >= w \/ ResumeAnyMode)

\* A rebuild of level w starts (or starts again after a crash). It either
\* carries on the owed record where its cursor stopped, or records a new one
\* of its own level whose cursor reads as the start.
RebuildStart(w) == /\ phase = "idle"
                   /\ phase' = "walking"
                   /\ want' = w
                   /\ IF Resumes(w)
                        THEN UNCHANGED <<cursor, owed, recLevel>>
                        ELSE /\ cursor' = 0
                             /\ owed' = TRUE
                             /\ recLevel' = w
                   /\ UNCHANGED level

\* A rebuild page walks the next document at the rebuild's own level.
RebuildPage == /\ phase = "walking"
               /\ cursor < N
               /\ cursor' = cursor + 1
               /\ Apply(cursor + 1, want)
               /\ UNCHANGED <<owed, recLevel, phase, want>>

\* A crash stops the rebuild: its record and cursor stay behind.
RebuildStop == /\ phase = "walking"
               /\ phase' = "idle"
               /\ UNCHANGED <<level, cursor, owed, recLevel, want>>

\* The rebuild's last page: past the last document, it clears the record and
\* reports success.
RebuildDone == /\ phase = "walking"
               /\ cursor = N
               /\ phase' = "done"
               /\ owed' = FALSE
               /\ cursor' = 0
               /\ UNCHANGED <<level, recLevel, want>>

\* Any one of the steps above.
Next == \/ SettlePage
        \/ SettleDone
        \/ \E w \in 1..2 : RebuildStart(w)
        \/ RebuildPage
        \/ RebuildStop
        \/ RebuildDone

\* The whole behaviour: start in Init, take Next steps (or stutter).
Spec == Init /\ [][Next]_vars

\* A rebuild that reports done has walked every document at least at the
\* level it asked for.
INV_RebuiltAll == phase = "done" => \A d \in Docs : level[d] >= want
================================================================================
