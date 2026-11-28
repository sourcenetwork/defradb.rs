---- MODULE TruncateUnderWriters ----
\* The story: an operator truncates a collection while clients keep updating
\* its documents. Truncate removes every document for good. The store is
\* regolith: every transaction reads one snapshot, and at commit it is refused
\* (a conflict) when a key it depends on was written after that snapshot. A
\* refused transaction runs again from a new snapshot, a bounded number of
\* times; after the last one the truncate gives up with an error.
\*
\* Who acts:
\*   - the truncate, one transaction at a time, following its plan;
\*   - writers, each updating one document: it reads the document's deletion
\*     marker, and if the document is still there it merges into the
\*     document's value; the update can also record its document as an owner
\*     of a block another document shares (a field block with the same bytes).
\*
\* What a commit is refused for (regolith at DefraLevel):
\*   - a delete or put of a key that someone wrote after the snapshot (the
\*     purge deletes the value writers merge into);
\*   - a merge into a key someone put or deleted after the snapshot;
\*   - a point read (the writer's read of the marker) of a key someone wrote
\*     after the snapshot, when that read is validated;
\*   - a validated scan (the purge's look for a shared block's other owners)
\*     of a range someone wrote into after the snapshot.
\*
\* Before the fix the truncate purged each chunk straight away: every writer
\* that committed during a purge attempt made it run again, so with steady
\* writers it never finished. The fix marks every document deleted first, in
\* transactions of their own that write only markers, which no update writes,
\* so nothing refuses them. Once a marker commits, a writer that read it absent
\* is refused and its re-run finds the document gone, so no update of that
\* document commits again. Only then are the chunks purged, and nothing a
\* writer does can refuse a purge.
\*
\* What must never happen: a truncate transaction refused because of an
\* update (INV_FirstAttemptCommits), so the truncate never runs out of
\* retries (INV_NeverExhausted); an update committing to a document after its
\* marker did (INV_NoWriteAfterMark); a finished truncate leaving a document
\* (INV_DoneRemovesAll).
\*
\* Creates, deletes and merges from peers can still refuse a purge; they are
\* outside this model, as in Truncate.tla's contention.
\*
\* Code it models:
\*   crates/database/db/src/collection/ops/truncate/all.rs
\*     DB::truncate_documents   the plan: every chunk marked, then every chunk purged
\*     DB::mark_chunk_deleted   a mark transaction (Mark)
\*     DB::truncate_chunk_in    a purge transaction (Purge): deletes the value and
\*                              marker, scans shared blocks' owners validated
\*                              (crates/database/db/src/block/cleanup.rs delete_owned_dag_for_owners)
\*   crates/database/db/src/collection/index.rs Collection::set_deletion_marker  the marker put
\*   crates/database/db/src/collection/crud.rs Collection::fields_at  the writer's validated
\*                              read of the marker (WBegin, WCommit)
\*   crates/database/db/src/write/autocommit/update.rs run_update_attempt  the update attempt
\*   crates/engine/storage/src/corekv/retry/policy.rs engine_retry_options  the bounded retries
EXTENDS Naturals, Sequences, FiniteSets   \* numbers, the plan as a sequence, and counting

CONSTANTS
  Docs,         \* the documents, one chunk each, for example {"d1", "d2"}
  Writers,      \* the clients updating documents, for example {"w1", "w2"}
  MaxCommits,   \* how many updates may commit in all, to keep the model finite
  MaxAttempts,  \* how many times one truncate transaction may run
  Enforced      \* which parts of the fix the code really has

\* Two documents at least, so one can be written while the other is purged.
ASSUME Cardinality(Docs) >= 2
\* The budgets are positive numbers.
ASSUME MaxCommits \in Nat /\ MaxAttempts \in Nat \ {0}
\* "mark": documents are marked deleted before they are purged;
\* "split": the marks commit in transactions of their own, before the purge
\*          begins (without it the marker is set in the purge's own transaction);
\* "markall": every chunk is marked before any is purged (without it a chunk
\*          is marked and then purged before the next is marked);
\* "validated": the writer's read of the marker is checked at its commit.
ASSUME Enforced \subseteq {"mark", "split", "markall", "validated"}

\* No commit has happened yet: sequence numbers start after this.
None == 0

VARIABLES
  seq,           \* the sequence number of the newest commit
  valueAt,       \* per document, the sequence of the newest write to its value
  valueKind,     \* per document, what that write was: "merge" or "delete"
  markerAt,      \* per document, the sequence of the newest write to its marker
  marked,        \* per document, the marker is present
  purged,        \* per document, the purge removed it
  ownersAt,      \* the sequence of the newest write into the shared owner range
  commits,       \* how many updates committed
  lateWrite,     \* an update committed to a document after its marker did
  plan,          \* the truncate's transactions still to run, in order
  tSnap,         \* the snapshot of the truncate's running attempt, or None between attempts
  attempts,      \* how many attempts of the current transaction were refused
  refusedOnce,   \* some truncate attempt was refused
  tphase,        \* "run", "done" or "failed"
  wDoc,          \* per writer, the document it is updating, or "" when idle
  wSnap,         \* per writer, the snapshot of its attempt
  wSawMarker     \* per writer, its attempt read the marker present

\* Every variable, for saying a step leaves them all as they are.
vars == <<seq, valueAt, valueKind, markerAt, marked, purged, ownersAt, commits,
          lateWrite, plan, tSnap, attempts, refusedOnce, tphase, wDoc, wSnap,
          wSawMarker>>

\* The variables a truncate step never touches: the writers' own state.
writerVars == <<wDoc, wSnap, wSawMarker, commits>>

\* One step of the plan: a kind ("mark", "purge" or, without "split",
\* "markpurge") and the documents of its chunk.
Step(kind, docs) == [kind |-> kind, docs |-> docs]

\* The documents in a fixed order, so the plan is a sequence: any order works.
DocSeq == CHOOSE s \in [1..Cardinality(Docs) -> Docs] :
            \A i, j \in 1..Cardinality(Docs) : i # j => s[i] # s[j]

\* The steps for one chunk when marking and purging come together.
ChunkSteps(d) ==
  IF "mark" \notin Enforced THEN << Step("purge", {d}) >>
  ELSE IF "split" \notin Enforced THEN << Step("markpurge", {d}) >>
  ELSE << Step("mark", {d}), Step("purge", {d}) >>

\* Every chunk's steps, one chunk after the other.
PerChunkPlan ==
  LET n == Cardinality(Docs)
      Build[i \in 0..n] == IF i = 0 THEN << >> ELSE Build[i - 1] \o ChunkSteps(DocSeq[i])
  IN Build[n]

\* Every chunk marked, then every chunk purged (the fix).
MarkAllPlan ==
  [i \in 1..Cardinality(Docs) |-> Step("mark", {DocSeq[i]})]
    \o [i \in 1..Cardinality(Docs) |-> Step("purge", {DocSeq[i]})]

\* The plan the code follows.
InitialPlan ==
  IF {"mark", "split", "markall"} \subseteq Enforced THEN MarkAllPlan ELSE PerChunkPlan

\* Every document exists, created by the commit at sequence 1; the truncate is
\* about to begin and every writer is idle.
Init ==
  /\ seq = 1
  /\ valueAt = [d \in Docs |-> 1]
  /\ valueKind = [d \in Docs |-> "merge"]
  /\ markerAt = [d \in Docs |-> None]
  /\ marked = [d \in Docs |-> FALSE]
  /\ purged = [d \in Docs |-> FALSE]
  /\ ownersAt = None
  /\ commits = 0
  /\ lateWrite = FALSE
  /\ plan = InitialPlan
  /\ tSnap = None
  /\ attempts = 0
  /\ refusedOnce = FALSE
  /\ tphase = "run"
  /\ wDoc = [w \in Writers |-> ""]
  /\ wSnap = [w \in Writers |-> None]
  /\ wSawMarker = [w \in Writers |-> FALSE]

\* ---------------------------------------------------------------- truncate

\* The truncate begins an attempt of its next transaction: it takes a snapshot.
TBegin ==
  /\ tphase = "run"
  /\ plan # << >>
  /\ tSnap = None
  /\ tSnap' = seq
  /\ UNCHANGED <<seq, valueAt, valueKind, markerAt, marked, purged, ownersAt,
                 lateWrite, plan, attempts, refusedOnce, tphase>>
  /\ UNCHANGED writerVars

\* A purge of `docs` from snapshot `snap` is refused when a value it deletes,
\* or the shared owner range it scans validated, was written after `snap`.
PurgeRefused(docs, snap) ==
  \/ \E d \in docs : valueAt[d] > snap
  \/ ownersAt > snap

\* A mark only puts markers. Local deletes and peer delete merges put markers
\* too, so a delete that commits during a mark attempt refuses it; a document
\* is deleted once, so every mark still finishes after a bounded retry.
MarkRefused(docs, snap) == \E d \in docs : markerAt[d] > snap

\* Whether the running attempt's commit is refused.
Refused(step) ==
  IF step.kind = "mark" THEN MarkRefused(step.docs, tSnap)
  ELSE PurgeRefused(step.docs, tSnap)

\* The attempt is refused: it runs again from a new snapshot, or after the
\* last attempt the truncate fails.
TRefused ==
  /\ tphase = "run"
  /\ tSnap # None
  /\ Refused(Head(plan))
  /\ refusedOnce' = TRUE
  /\ tSnap' = None
  /\ IF attempts + 1 >= MaxAttempts
       THEN /\ tphase' = "failed"
            /\ attempts' = attempts + 1
       ELSE /\ attempts' = attempts + 1
            /\ UNCHANGED tphase
  /\ UNCHANGED <<seq, valueAt, valueKind, markerAt, marked, purged, ownersAt,
                 lateWrite, plan>>
  /\ UNCHANGED writerVars

\* The attempt commits: a mark sets the markers, a purge deletes the values
\* and markers; either way one new sequence number.
TCommit ==
  /\ tphase = "run"
  /\ tSnap # None
  /\ ~Refused(Head(plan))
  /\ LET step == Head(plan)
         next == seq + 1
     IN /\ seq' = next
        /\ IF step.kind = "mark"
             THEN /\ markerAt' = [d \in Docs |-> IF d \in step.docs THEN next ELSE markerAt[d]]
                  /\ marked' = [d \in Docs |-> marked[d] \/ d \in step.docs]
                  /\ UNCHANGED <<valueAt, valueKind, purged>>
             ELSE /\ valueAt' = [d \in Docs |-> IF d \in step.docs THEN next ELSE valueAt[d]]
                  /\ valueKind' = [d \in Docs |-> IF d \in step.docs THEN "delete" ELSE valueKind[d]]
                  /\ markerAt' = [d \in Docs |-> IF d \in step.docs THEN next ELSE markerAt[d]]
                  /\ marked' = [d \in Docs |-> marked[d] /\ d \notin step.docs]
                  /\ purged' = [d \in Docs |-> purged[d] \/ d \in step.docs]
        /\ plan' = Tail(plan)
        /\ tSnap' = None
        /\ attempts' = 0
        /\ tphase' = IF Tail(plan) = << >> THEN "done" ELSE "run"
  /\ UNCHANGED <<ownersAt, lateWrite, refusedOnce>>
  /\ UNCHANGED writerVars

\* ----------------------------------------------------------------- writers

\* Writer `w` begins an update of document `d`: it takes a snapshot and reads
\* the marker; a purged document is not found, so it does not start.
WBegin(w, d) ==
  /\ wDoc[w] = ""
  /\ commits < MaxCommits
  /\ ~purged[d]
  /\ wDoc' = [wDoc EXCEPT ![w] = d]
  /\ wSnap' = [wSnap EXCEPT ![w] = seq]
  /\ wSawMarker' = [wSawMarker EXCEPT ![w] = marked[d]]
  /\ UNCHANGED <<seq, valueAt, valueKind, markerAt, marked, purged, ownersAt,
                 commits, lateWrite, plan, tSnap, attempts, refusedOnce, tphase>>

\* The update's commit is refused: its validated marker read changed, or the
\* value it merges into was deleted after its snapshot.
WRefused(w) ==
  LET d == wDoc[w] IN
  \/ "validated" \in Enforced /\ markerAt[d] > wSnap[w]
  \/ valueKind[d] = "delete" /\ valueAt[d] > wSnap[w]

\* The writer ends its attempt: it read the marker present (the document is
\* gone, a race it may lose), or its commit is refused (it begins again).
WGiveUp(w) ==
  /\ wDoc[w] # ""
  /\ wSawMarker[w] \/ WRefused(w)
  /\ wDoc' = [wDoc EXCEPT ![w] = ""]
  /\ UNCHANGED <<wSnap, wSawMarker, commits>>
  /\ UNCHANGED <<seq, valueAt, valueKind, markerAt, marked, purged, ownersAt,
                 lateWrite, plan, tSnap, attempts, refusedOnce, tphase>>

\* The update commits: it merges into the value and, when `shared`, records
\* its document as an owner of a block another document shares.
WCommit(w, shared) ==
  /\ wDoc[w] # ""
  /\ ~wSawMarker[w]
  /\ ~WRefused(w)
  /\ LET d == wDoc[w]
         next == seq + 1
     IN /\ seq' = next
        /\ valueAt' = [valueAt EXCEPT ![d] = next]
        /\ valueKind' = [valueKind EXCEPT ![d] = "merge"]
        /\ ownersAt' = IF shared THEN next ELSE ownersAt
        /\ lateWrite' = (lateWrite \/ marked[d])
  /\ commits' = commits + 1
  /\ wDoc' = [wDoc EXCEPT ![w] = ""]
  /\ UNCHANGED <<wSnap, wSawMarker>>
  /\ UNCHANGED <<markerAt, marked, purged, plan, tSnap, attempts, refusedOnce, tphase>>

\* Some actor takes a step.
Next ==
  \/ TBegin
  \/ TRefused
  \/ TCommit
  \/ \E w \in Writers, d \in Docs : WBegin(w, d)
  \/ \E w \in Writers : WGiveUp(w)
  \/ \E w \in Writers, shared \in BOOLEAN : WCommit(w, shared)

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* -------------------------------------------------------------- properties

\* Every variable holds a value of the expected kind.
TypeOK ==
  /\ seq \in Nat
  /\ valueKind \in [Docs -> {"merge", "delete"}]
  /\ marked \in [Docs -> BOOLEAN]
  /\ purged \in [Docs -> BOOLEAN]
  /\ tphase \in {"run", "done", "failed"}
  /\ attempts \in 0..MaxAttempts
  /\ wDoc \in [Writers -> Docs \cup {""}]

\* Promise: no truncate transaction is ever refused because of an update.
INV_FirstAttemptCommits == ~refusedOnce

\* Promise: the truncate never runs out of retries while clients update.
INV_NeverExhausted == tphase # "failed"

\* Promise: once a document's marker committed, no update of it commits.
INV_NoWriteAfterMark == ~lateWrite

\* Promise: a finished truncate removed every document, and nothing an update
\* wrote came after its document's purge.
INV_DoneRemovesAll ==
  tphase = "done" => \A d \in Docs : purged[d] /\ valueKind[d] = "delete"

====
