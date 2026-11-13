-------------------- MODULE TransactionalRelationship --------------------
\* A caller stages an authorized relationship grant in its own writable
\* transaction. A separate publisher can finish the marker and revoke the
\* grant before the first publisher commits. The commit must validate its
\* marker read and must never restore the revoked grant. The model abstracts
\* both the marker write and all relationship writes as one transaction.
\* It models query's transaction turn and ACP binding, and the HTTP edge.
EXTENDS Naturals

CONSTANT Bug                        \* Each RED config removes one guard.
VARIABLES pending, version, grant, staged, seen, leaked, bad, revoked, revived
vars == <<pending, version, grant, staged, seen, leaked, bad, revoked, revived>>

Init ==                             \* An unpublished request has no grant.
  /\ pending = TRUE                 \* Its publication marker is pending.
  /\ version = 0                    \* No other publisher has changed it.
  /\ grant = FALSE                  \* No relationship is visible.
  /\ staged = FALSE                 \* This publisher has done nothing.
  /\ seen = 0                       \* No marker version has been observed.
  /\ leaked = FALSE                 \* No uncommitted effect is visible.
  /\ bad = FALSE                    \* No unauthorized effect was staged.
  /\ revoked = FALSE                \* The completed grant was not revoked.
  /\ revived = FALSE                \* A revoked grant was not restored.

Stage(owner, writable, nac, dac, bound, fresh, backend) ==
  /\ pending /\ ~staged             \* This publisher sees a pending marker.
  /\ LET allowed ==                 \* Every authority and binding is required.
       (owner \/ Bug = "Owner") /\ (writable \/ Bug = "Readonly") /\
       (nac \/ Bug = "Nac") /\ (dac \/ Bug = "Dac") /\
       (bound \/ Bug = "Provider") /\ (fresh \/ Bug = "Expiry") /\
       (backend \/ Bug = "Backend")
     IN /\ staged' = allowed        \* A denied request stages nothing.
        /\ bad' = (bad \/ (allowed /\ ~(owner /\ writable /\ nac /\ dac /\ bound /\ fresh /\ backend)))
  /\ seen' = version                \* Commit validates the observed version.
  /\ IF Bug = "Autocommit" /\ staged' THEN
       /\ grant' = TRUE             \* The broken edge publishes too early.
       /\ leaked' = TRUE
     ELSE /\ UNCHANGED <<grant, leaked>>
  /\ UNCHANGED <<pending, version, revoked, revived>>

OtherPublisher ==                   \* Another publisher wins the race.
  /\ pending
  /\ pending' = FALSE               \* Its marker and grants commit together.
  /\ version' = 1
  /\ grant' = TRUE
  /\ UNCHANGED <<staged, seen, leaked, bad, revoked, revived>>

Revoke ==                           \* A later explicit revocation wins.
  /\ ~pending /\ grant
  /\ grant' = FALSE
  /\ revoked' = TRUE
  /\ UNCHANGED <<pending, version, staged, seen, leaked, bad, revived>>

Commit ==                           \* Commit is atomic with marker transition.
  /\ staged
  /\ IF seen = version \/ Bug = "Conflict" THEN
       /\ grant' = TRUE
       /\ pending' = FALSE
       /\ version' = 1
       /\ revived' = (revived \/ revoked)
     ELSE /\ UNCHANGED <<grant, pending, version, revived>>
  /\ staged' = FALSE
  /\ UNCHANGED <<seen, leaked, bad, revoked>>

Discard ==                          \* Discard leaves committed state alone.
  /\ staged
  /\ staged' = FALSE
  /\ UNCHANGED <<pending, version, grant, seen, leaked, bad, revoked, revived>>

Next ==                             \* Explore every interleaving and caller.
  (\E owner, writable, nac, dac, bound, fresh, backend \in BOOLEAN : Stage(owner, writable, nac, dac, bound, fresh, backend))
  \/ OtherPublisher \/ Revoke \/ Commit \/ Discard
Spec == Init /\ [][Next]_vars

INV_Authority == ~bad               \* Refused requests never stage effects.
INV_Private == ~leaked              \* A transaction never autocommits.
INV_NoRevival == ~revived           \* A stale publisher never restores a revoke.
=============================================================================
