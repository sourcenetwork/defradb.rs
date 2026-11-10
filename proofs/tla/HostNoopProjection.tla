------------------------ MODULE HostNoopProjection ------------------------
\* A host replays an existing root while a dependent grant may be missing.
\* Reconciliation uses the operator's authority and commits atomically.
EXTENDS Naturals
\* Each red run removes one guard from the correct operation.
CONSTANT Bug
\* The root already exists. The descendant and transaction have their own state.
VARIABLES child, active, admin, config, unauthorized
\* The complete state, for stuttering and temporal checking.
vars == <<child, active, admin, config, unauthorized>>
\* Either the descendant is already present or it must be repaired.
Init == /\ child \in BOOLEAN /\ active = FALSE /\ admin = FALSE
        /\ config = FALSE /\ unauthorized = FALSE
\* The actual operator starts the transaction.
Start(a) == /\ ~active /\ ~config /\ active' = TRUE /\ admin' = a
            /\ UNCHANGED <<child, config, unauthorized>>
\* A successful replay checks the descendant and creates it only if authorized.
Finish == /\ active
          /\ (child \/ admin \/ Bug = "Skipped" \/ Bug = "Authority")
          /\ child' = (IF Bug = "Skipped" THEN child ELSE TRUE)
          /\ unauthorized' = (unauthorized \/ (~admin /\ ~child /\ child'))
          /\ config' = TRUE /\ active' = FALSE
          /\ UNCHANGED admin
\* A refused dependent change rolls the entire transaction back.
Abort == /\ active /\ active' = FALSE
         /\ config' = (IF Bug = "Rollback" THEN TRUE ELSE config)
         /\ UNCHANGED <<child, admin, unauthorized>>
\* Both ordinary actors and administrators can try a replay.
Next == (\E a \in BOOLEAN : Start(a)) \/ Finish \/ Abort
\* A transaction may wait between steps.
Spec == Init /\ [][Next]_vars
\* A reported successful configuration has its required descendant.
INV_Ready == ~config \/ child
\* An unprivileged operator never creates a new dependent grant.
INV_Authority == ~unauthorized
=============================================================================
