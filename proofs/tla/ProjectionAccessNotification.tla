----------------- MODULE ProjectionAccessNotification -----------------
EXTENDS Naturals
\* A bug selects exactly one omitted notification or leaked callback.
CONSTANT Bug
\* Current access, last observed access, pending wake, and leaked abort/no-op.
VARIABLES current, observed, pending, leakedAbort, leakedNoop
\* The state used by the temporal specification.
vars == <<current, observed, pending, leakedAbort, leakedNoop>>
\* The observer starts armed and agrees with native access.
Init == /\ current = FALSE /\ observed = FALSE /\ pending = FALSE
        /\ leakedAbort = FALSE /\ leakedNoop = FALSE
\* A committed grant queues a wake for the changed document.
Grant == /\ ~current /\ current' = TRUE
         /\ pending' = IF Bug = "Grant" THEN pending ELSE TRUE
         /\ UNCHANGED <<observed, leakedAbort, leakedNoop>>
\* A committed revoke also queues a wake.
Revoke == /\ current /\ current' = FALSE
          /\ pending' = IF Bug = "Revoke" THEN pending ELSE TRUE
          /\ UNCHANGED <<observed, leakedAbort, leakedNoop>>
\* An aborted relation change does not publish a callback.
Abort == /\ leakedAbort' = IF Bug = "Abort" THEN TRUE ELSE leakedAbort
         /\ UNCHANGED <<current, observed, pending, leakedNoop>>
\* A no-op relation change need not wake an observer.
Noop == /\ leakedNoop' = IF Bug = "Noop" THEN TRUE ELSE leakedNoop
        /\ UNCHANGED <<current, observed, pending, leakedAbort>>
\* An armed observer consumes the invalidation and re-reads current access.
Observe == /\ pending /\ observed' = current /\ pending' = FALSE
           /\ UNCHANGED <<current, leakedAbort, leakedNoop>>
\* The environment may commit, abort, or re-read in any order.
Next == Grant \/ Revoke \/ Abort \/ Noop \/ Observe
\* Infinite stuttering keeps the finite transition system total.
Spec == Init /\ [][Next]_vars
\* A stale observer always has an outstanding invalidation.
INV_Wake == current = observed \/ pending
\* Aborted work never produces a notification.
INV_Abort == ~leakedAbort
\* Unchanged work never produces a notification.
INV_Noop == ~leakedNoop
=============================================================================
