---- MODULE LocalFetchPermit ----
\* A clock-dispatched fetch owns a source-peer permit. The root may already
\* be complete when the fetch starts, so it queues a merge event without
\* issuing any request. The merge channel may be full. A second fetch for
\* the same peer must be able to take the permit while the first waits. This
\* model does not claim availability of the separate global permit.
\*
\* State correspondence (crates/p2p/src/sync/coordinator/dag_fetcher.rs):
\* phase is poll_fetch_dag's program counter; held is the Option in its
\* DagFetchContext; otherHeld is a second task's semaphore permit; full is
\* whether event_tx's bounded channel has no room.
EXTENDS Naturals

CONSTANT ReleaseBeforeSend
VARIABLES phase, held, otherHeld, full
vars == <<phase, held, otherHeld, full>>

\* The clock reserved the only peer permit, and the channel is full.
Init == /\ phase = "Check"
        /\ held = TRUE
        /\ otherHeld = FALSE
        /\ full = TRUE

\* Locally complete: take and drop the permit before beginning the send.
LocalComplete == /\ phase = "Check"
                 /\ phase' = "Send"
                 /\ held' = IF ReleaseBeforeSend THEN FALSE ELSE held
                 /\ UNCHANGED <<otherHeld, full>>

\* The receiver can make room in the merge channel.
Drain == /\ full
         /\ full' = FALSE
         /\ UNCHANGED <<phase, held, otherHeld>>

\* The first fetch sends its event and ends; drop releases any held permit.
Send == /\ phase = "Send" /\ ~full
        /\ phase' = "Done" /\ held' = FALSE /\ full' = TRUE
        /\ UNCHANGED otherHeld

\* Another fetch can take the one peer permit when it is available.
AcquireOther == /\ ~held /\ ~otherHeld
                /\ otherHeld' = TRUE
                /\ UNCHANGED <<phase, held, full>>

\* The other fetch can finish and return its permit.
ReleaseOther == /\ otherHeld
                /\ otherHeld' = FALSE
                /\ UNCHANGED <<phase, held, full>>

Next == LocalComplete \/ Drain \/ Send \/ AcquireOther \/ ReleaseOther
Spec == Init /\ [][Next]_vars

\* Waiting to send a completed DAG never holds its old network permit.
NoPermitWhileSending == phase = "Send" => ~held
\* A semaphore permit can have at most one owner.
OnePermit == ~(held /\ otherHeld)
====
