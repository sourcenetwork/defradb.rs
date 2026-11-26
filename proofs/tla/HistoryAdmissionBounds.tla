------------------------- MODULE HistoryAdmissionBounds -------------------------
\* The story. Two merges arrive at a node that lets one merge onto its worker
\* pool at a time. Merge 1 may get stuck there forever (a slow inner merge).
\* Merge 2 then waits for the slot. Its wait carries a deadline: when the
\* deadline passes it gives up with a typed retryable error and is delivered
\* again later. A waiting merge may also be dropped (its caller went away);
\* that must not leak the slot.
\* Separately, the durable history admission index may hold more entries than
\* its capacity (a store from an older node, or a lowered limit). A new history
\* root wants in. Each attempt reads at most Cap entries: at capacity it
\* reclaims one idle entry and admits nothing; below capacity it admits.
\* What must never happen: a merge waits forever for a slot; a dropped wait
\* keeps a slot; an over-full index never drains; a root is admitted while
\* the index is at capacity.
\* Rust: crates/database/db/src/merge/pooled.rs (PooledMergeHandler::admit,
\* handle_block, handle_block_batch); crates/database/db/src/merge/merge_handler/
\* history_registry.rs (live_roots, reclaim_idle) and history_walk.rs
\* (DbMergeHandler::history_page, its admission branch).
EXTENDS Naturals, FiniteSets

\* Which guard a RED configuration removes; "none" keeps every guard.
CONSTANT Bug

\* The two merges that compete for the pool's admission slot.
Merges == {1, 2}
\* Merge 1 is the one that may never finish once it runs.
Stuck == 1
\* One admission slot: the smallest pool that still makes a merge wait.
Permits == 1
\* A wait gives up after this many clock ticks.
Deadline == 2

\* Durable admission index entries; the index starts one over its capacity.
Entries == {1, 2, 3}
\* The index's capacity (MAX_ROOTS in Rust).
Cap == 2
\* The new root that asks to be admitted.
NewRoot == 4

\* stage: where each merge is; free: open slots; waited: ticks a waiter saw.
\* index: the durable admission entries; overAdmitted: an admission at capacity happened.
VARIABLES stage, free, waited, index, overAdmitted
\* All the state in one tuple for UNCHANGED and fairness.
vars == <<stage, free, waited, index, overAdmitted>>

\* No merge has started, every slot is open, the index is over full.
Init == /\ stage = [m \in Merges |-> "idle"]
        /\ free = Permits
        /\ waited = [m \in Merges |-> 0]
        /\ index = Entries
        /\ overAdmitted = FALSE

\* A merge asks for a slot: it runs at once if one is open, else it waits.
Arrive(m) == /\ stage[m] = "idle"
             /\ IF free > 0
                THEN /\ stage' = [stage EXCEPT ![m] = "running"]
                     /\ free' = free - 1
                ELSE /\ stage' = [stage EXCEPT ![m] = "waiting"]
                     /\ UNCHANGED free
             /\ UNCHANGED <<waited, index, overAdmitted>>

\* A waiter takes a slot that opened while it waited.
Grant(m) == /\ stage[m] = "waiting" /\ free > 0
            /\ stage' = [stage EXCEPT ![m] = "running"]
            /\ free' = free - 1
            /\ UNCHANGED <<waited, index, overAdmitted>>

\* The waiter's deadline timer ticks; without a deadline there is no timer.
Tick(m) == /\ stage[m] = "waiting" /\ Bug # "nodeadline"
           /\ waited[m] < Deadline
           /\ waited' = [waited EXCEPT ![m] = @ + 1]
           /\ UNCHANGED <<stage, free, index, overAdmitted>>

\* The deadline passed: the wait ends with a typed retryable error, no slot held.
TimeOut(m) == /\ stage[m] = "waiting" /\ Bug # "nodeadline"
              /\ waited[m] = Deadline
              /\ stage' = [stage EXCEPT ![m] = "retry"]
              /\ UNCHANGED <<free, waited, index, overAdmitted>>

\* The caller drops a waiting merge; the semaphore forgets the waiter.
\* With Bug = "leak" the dropped waiter's place still consumes a slot.
Cancel(m) == /\ stage[m] = "waiting"
             /\ stage' = [stage EXCEPT ![m] = "cancelled"]
             /\ free' = IF Bug = "leak" /\ free > 0 THEN free - 1 ELSE free
             /\ UNCHANGED <<waited, index, overAdmitted>>

\* A merge that is not stuck finishes and gives its slot back.
Release(m) == /\ stage[m] = "running" /\ m # Stuck
              /\ stage' = [stage EXCEPT ![m] = "done"]
              /\ free' = free + 1
              /\ UNCHANGED <<waited, index, overAdmitted>>

\* The entries one attempt reads: at most Cap of them (a capped, validated scan).
Visible == IF Cardinality(index) > Cap
           THEN CHOOSE s \in SUBSET index : Cardinality(s) = Cap
           ELSE index

\* One admission attempt for the new root. With Bug = "failfirst" an index
\* over capacity fails the attempt before reclamation can run, as the old
\* scan did. At capacity the attempt reclaims one idle visible entry (every
\* existing entry is idle here). Below capacity it admits the new root; with
\* Bug = "admitfull" it admits even at capacity.
Attempt == /\ NewRoot \notin index
           /\ IF Bug = "failfirst" /\ Cardinality(index) > Cap
              THEN UNCHANGED <<index, overAdmitted>>
              ELSE IF Cardinality(Visible) >= Cap /\ Bug # "admitfull"
                   THEN /\ \E e \in Visible : index' = index \ {e}
                        /\ UNCHANGED overAdmitted
                   ELSE /\ index' = index \cup {NewRoot}
                        /\ overAdmitted' = (Cardinality(index) >= Cap)
           /\ UNCHANGED <<stage, free, waited>>

\* Every schedule of arrivals, grants, ticks, timeouts, drops, releases and attempts.
Next == \/ \E m \in Merges : Arrive(m) \/ Grant(m) \/ Tick(m) \/ TimeOut(m)
                              \/ Cancel(m) \/ Release(m)
        \/ Attempt

\* The clock, grants, releases and admission attempts keep running; drops may never happen.
Fairness == /\ \A m \in Merges : WF_vars(Grant(m)) /\ WF_vars(Tick(m))
                                 /\ WF_vars(TimeOut(m)) /\ WF_vars(Release(m))
            /\ WF_vars(Attempt)

\* The whole behaviour: start, then any steps, with the fairness above.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every slot is either open or held by a running merge: drops leak none.
INV_Permits == free + Cardinality({m \in Merges : stage[m] = "running"}) = Permits
\* No root is admitted while the index is at or over capacity.
INV_NoOverAdmit == ~overAdmitted
\* A waiting merge always leaves the wait: it runs, times out, or is dropped.
LIVE_WaitEnds == \A m \in Merges : (stage[m] = "waiting") ~> (stage[m] # "waiting")
\* The over-full index drains until the new root is admitted.
LIVE_Recovers == <>(NewRoot \in index)
=============================================================================
