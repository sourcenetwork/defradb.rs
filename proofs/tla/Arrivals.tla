---- MODULE Arrivals ----
\* Per-collection arrival log: crates/db/src/event/arrivals.rs. A consumer polls
\* `read(after)` for the documents this node committed past its cursor and moves
\* its cursor to the page's `next`.
\*
\* Concurrent writers each create a distinct document, so a conflict between any
\* two of them is a conflict between disjoint documents. Collection-level state
\* must commute (as the branchable head set does since #1211); per-document state
\* conflicts as usual.
\*
\* Mode selects how a committed arrival gets its cursor:
\*   "Counter"   - head key bumped inside the document txn (main before #1897).
\*                 Disjoint creates conflict on the head key.            [RED]
\*   "Detached"  - pending key in the document txn, numbered by a second
\*                 txn after commit, in short-ID order (#1897). A second
\*                 durable commit per write, and cursors leave commit
\*                 order.                                                [RED]
\*   "Stamp"     - the cursor is the engine commit sequence, filled into
\*                 the row key at commit (sourcenetwork/regolith#216), and
\*                 a reader's bound is its snapshot's published horizon. [GREEN]
\*   "StampLatest" - as Stamp, but the reader bounds by the latest
\*                 allocated sequence instead of the published horizon. A
\*                 commit allocated below the bound but not yet applied is
\*                 skipped for good.                                     [RED]
EXTENDS Naturals, FiniteSets

CONSTANTS Docs, Mode

ASSUME Mode \in {"Counter", "Detached", "Stamp", "StampLatest"}

Stamped == Mode \in {"Stamp", "StampLatest"}

VARIABLES
  pc,         \* [Docs -> phase]
  shortId,    \* [Docs -> Nat] allocated at begin, outside the txn (docid/map.rs)
  nextShort,
  snapHead,   \* [Docs -> Nat] head the txn read (Counter)
  head,       \* committed head key (Counter, Detached)
  pending,    \* committed, unnumbered arrivals (Detached)
  seq,        \* [Docs -> Nat] commit sequence (Stamp*)
  allocated,  \* engine latest_seq
  visible,    \* engine published horizon
  log,        \* set of [doc, cur]
  commitIdx,  \* [Docs -> Nat] oracle: position in commit order
  commitCount,
  durable,    \* durable commits issued
  rcur,       \* reader cursor
  seen        \* docs the reader was handed

vars == <<pc, shortId, nextShort, snapHead, head, pending, seq, allocated,
          visible, log, commitIdx, commitCount, durable, rcur, seen>>

Init ==
  /\ pc = [d \in Docs |-> "idle"]
  /\ shortId = [d \in Docs |-> 0]
  /\ nextShort = 1
  /\ snapHead = [d \in Docs |-> 0]
  /\ head = 0
  /\ pending = {}
  /\ seq = [d \in Docs |-> 0]
  /\ allocated = 0
  /\ visible = 0
  /\ log = {}
  /\ commitIdx = [d \in Docs |-> 0]
  /\ commitCount = 0
  /\ durable = 0
  /\ rcur = 0
  /\ seen = {}

Begin(d) ==
  /\ pc[d] = "idle"
  /\ pc' = [pc EXCEPT ![d] = "open"]
  /\ shortId' = [shortId EXCEPT ![d] = nextShort]
  /\ nextShort' = nextShort + 1
  /\ snapHead' = [snapHead EXCEPT ![d] = head]
  /\ UNCHANGED <<head, pending, seq, allocated, visible, log, commitIdx,
                 commitCount, durable, rcur, seen>>

Committed(d) ==
  /\ commitIdx' = [commitIdx EXCEPT ![d] = commitCount + 1]
  /\ commitCount' = commitCount + 1
  /\ durable' = durable + 1

\* Optimistic commit: the head key read at begin must be unchanged.
CounterCommit(d) ==
  /\ Mode = "Counter"
  /\ pc[d] = "open"
  /\ IF snapHead[d] = head
       THEN /\ head' = head + 1
            /\ log' = log \cup {[doc |-> d, cur |-> head + 1]}
            /\ pc' = [pc EXCEPT ![d] = "done"]
            /\ Committed(d)
       ELSE /\ pc' = [pc EXCEPT ![d] = "aborted"]
            /\ UNCHANGED <<head, log, commitIdx, commitCount, durable>>
  /\ UNCHANGED <<shortId, nextShort, snapHead, pending, seq, allocated,
                 visible, rcur, seen>>

\* The document txn writes only its own pending key, so it never conflicts.
DetachedCommit(d) ==
  /\ Mode = "Detached"
  /\ pc[d] = "open"
  /\ pending' = pending \cup {d}
  /\ pc' = [pc EXCEPT ![d] = "done"]
  /\ Committed(d)
  /\ UNCHANGED <<shortId, nextShort, snapHead, head, seq, allocated, visible,
                 log, rcur, seen>>

\* One sequencing txn under the arrival guard: number every pending arrival
\* in short-ID order, then commit durably.
Rank(d) == Cardinality({p \in pending : shortId[p] <= shortId[d]})

Sequence ==
  /\ Mode = "Detached"
  /\ pending # {}
  /\ log' = log \cup {[doc |-> p, cur |-> head + Rank(p)] : p \in pending}
  /\ head' = head + Cardinality(pending)
  /\ pending' = {}
  /\ durable' = durable + 1
  /\ UNCHANGED <<pc, shortId, nextShort, snapHead, seq, allocated, visible,
                 commitIdx, commitCount, rcur, seen>>

\* run_group: take a sequence (latest_seq.fetch_add) ...
StampAllocate(d) ==
  /\ Stamped
  /\ pc[d] = "open"
  /\ allocated' = allocated + 1
  /\ seq' = [seq EXCEPT ![d] = allocated + 1]
  /\ pc' = [pc EXCEPT ![d] = "applying"]
  /\ UNCHANGED <<shortId, nextShort, snapHead, head, pending, visible, log,
                 commitIdx, commitCount, durable, rcur, seen>>

\* ... then WAL, apply, and publish the horizon, in sequence order.
StampApply(d) ==
  /\ Stamped
  /\ pc[d] = "applying"
  /\ seq[d] = visible + 1
  /\ log' = log \cup {[doc |-> d, cur |-> seq[d]]}
  /\ visible' = seq[d]
  /\ pc' = [pc EXCEPT ![d] = "done"]
  /\ Committed(d)
  /\ UNCHANGED <<shortId, nextShort, snapHead, head, pending, seq, allocated,
                 rcur, seen>>

Bound ==
  CASE Mode \in {"Counter", "Detached"} -> head
    [] Mode = "Stamp"                  -> visible
    [] Mode = "StampLatest"            -> allocated

\* read(after = rcur): every row in (rcur, bound], then next = bound.
Read ==
  /\ Bound > rcur
  /\ seen' = seen \cup {e.doc : e \in {e \in log : e.cur > rcur /\ e.cur <= Bound}}
  /\ rcur' = Bound
  /\ UNCHANGED <<pc, shortId, nextShort, snapHead, head, pending, seq,
                 allocated, visible, log, commitIdx, commitCount, durable>>

Next ==
  \/ \E d \in Docs : Begin(d) \/ CounterCommit(d) \/ DetachedCommit(d)
                     \/ StampAllocate(d) \/ StampApply(d)
  \/ Sequence
  \/ Read

Spec == Init /\ [][Next]_vars

\* A cursor the reader passed never gains a row at or below it.
INV_NoSkip == \A e \in log : e.cur <= rcur => e.doc \in seen

INV_UniqueCursors == \A a, b \in log : a.cur = b.cur => a = b

INV_CommitOrder ==
  \A a, b \in log : a.cur < b.cur => commitIdx[a.doc] < commitIdx[b.doc]

\* Disjoint documents never conflict on collection-level state.
INV_NoDisjointConflict == \A d \in Docs : pc[d] # "aborted"

\* A write costs exactly its own durable commit.
INV_OneCommitPerWrite == durable = commitCount

INV_Green ==
  /\ INV_NoSkip
  /\ INV_UniqueCursors
  /\ INV_CommitOrder
  /\ INV_NoDisjointConflict
  /\ INV_OneCommitPerWrite
====
