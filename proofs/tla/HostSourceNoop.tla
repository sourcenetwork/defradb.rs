--------------------------- MODULE HostSourceNoop ---------------------------
\* A trusted host may leave an identical source edge alone without a grant.
\* A real add or removal requires the actor's native administration rights.
\* A concurrent edge change invalidates the observation before commit.
EXTENDS Naturals
\* Bug removes one guard for its corresponding red model.
CONSTANT Bug
\* The transaction retains the edge and version it observed at its start.
VARIABLES edge, version, active, seen, seenVersion, desired, authorized, badAuth, badStale
\* These values are the complete state of the small transaction model.
vars == <<edge, version, active, seen, seenVersion, desired, authorized, badAuth, badStale>>
\* Start with no edge, no pending transaction and no violated obligation.
Init == /\ edge = FALSE /\ version = 0 /\ active = FALSE
        /\ seen = FALSE /\ seenVersion = 0 /\ desired = FALSE
        /\ authorized = FALSE /\ badAuth = FALSE /\ badStale = FALSE
\* Read the edge from the transaction and stage only a no-op or an authorized change.
Start(want, canManage) ==
  /\ ~active
  /\ want = edge \/ canManage \/ Bug = "Authority"
  /\ active' = TRUE /\ seen' = edge /\ seenVersion' = version
  /\ desired' = want /\ authorized' = canManage
  /\ badAuth' = badAuth \/ (~canManage /\ want # edge)
  /\ UNCHANGED <<edge, version, badStale>>
\* Another transaction may change the observed edge before this one commits.
OtherChange == /\ active /\ edge' = ~edge /\ version' = 1 - version
               /\ UNCHANGED <<active, seen, seenVersion, desired, authorized, badAuth, badStale>>
\* A validated observation lets the enclosing transaction commit.
Commit == /\ active /\ (seenVersion = version \/ Bug = "Validation")
          /\ badStale' = badStale \/ (~authorized /\ desired = seen /\ desired # edge)
          /\ edge' = desired /\ version' = version /\ active' = FALSE
          /\ UNCHANGED <<seen, seenVersion, desired, authorized, badAuth>>
\* A conflicted or discarded transaction leaves the current edge alone.
Discard == /\ active /\ active' = FALSE
           /\ UNCHANGED <<edge, version, seen, seenVersion, desired, authorized, badAuth, badStale>>
\* Explore both edge values and both kinds of actor.
Next == (\E want, canManage \in BOOLEAN : Start(want, canManage))
        \/ OtherChange \/ Commit \/ Discard
\* Real transactions may also pause between steps.
Spec == Init /\ [][Next]_vars
\* No unauthorised actor stages a new edge or removes an existing one.
INV_Authority == ~badAuth
\* A stale no-op cannot authorize a configuration against a changed edge.
INV_Validation == ~badStale
=============================================================================
