-------------------- MODULE UsersetRelationManager --------------------
EXTENDS Naturals
\* Each RED variant changes one authority or conflict guard.
CONSTANT Bug
\* Record a decision mismatch, membership version, staged grant and conflict.
VARIABLES mismatch, adminLive, version, staged, seen, stale
\* State for the temporal specification.
vars == <<mismatch, adminLive, version, staged, seen, stale>>
\* A manager membership starts active and no grant is staged.
Init == /\ mismatch = FALSE /\ adminLive = TRUE /\ version = 0
        /\ staged = FALSE /\ seen = 0 /\ stale = FALSE
\* Compare the actual decision against the declared, exact-Node membership.
Attempt(owner, declared, direct, wildcard, same, nodeOwner, nodeAdmin) ==
  LET expected == owner \/ (declared /\ (direct \/ wildcard \/
        (same /\ (nodeOwner \/ (nodeAdmin /\ adminLive)))))
      actual == (owner /\ Bug # "Owner") \/
        ((declared \/ Bug = "Target") /\
          ((direct /\ Bug # "Direct") \/ (wildcard /\ Bug # "Wildcard") \/
           ((same \/ Bug = "Node") /\
             ((nodeOwner /\ Bug # "NodeOwner") \/
              (nodeAdmin /\ adminLive /\ Bug # "NodeAdmin")))))
  IN /\ mismatch' = mismatch \/ (actual # expected)
     /\ UNCHANGED <<adminLive, version, staged, seen, stale>>
\* A current administrator stages a relationship change in its transaction.
Stage == /\ adminLive /\ ~staged /\ staged' = TRUE /\ seen' = version
         /\ UNCHANGED <<mismatch, adminLive, version, stale>>
\* Revocation changes the native relationship read by that transaction.
Revoke == /\ adminLive /\ adminLive' = FALSE /\ version' = 1
          /\ UNCHANGED <<mismatch, staged, seen, stale>>
\* A conflicting commit must not publish the staged grant.
Commit == /\ staged /\ staged' = FALSE
          /\ stale' = stale \/ (seen # version /\ Bug = "Conflict")
          /\ UNCHANGED <<mismatch, adminLive, version, seen>>
\* Discard abandons the staged grant.
Discard == /\ staged /\ staged' = FALSE
           /\ UNCHANGED <<mismatch, adminLive, version, seen, stale>>
\* The environment may request any authority combination or race revocation.
Next == (\E owner, declared, direct, wildcard, same, nodeOwner, nodeAdmin \in BOOLEAN :
           Attempt(owner, declared, direct, wildcard, same, nodeOwner, nodeAdmin))
        \/ Stage \/ Revoke \/ Commit \/ Discard
\* Stuttering completes the finite state machine.
Spec == Init /\ [][Next]_vars
\* Native membership must agree with the exact declared relationship.
INV_Membership == ~mismatch
\* A concurrent revocation cannot authorize a committed stale grant.
INV_Revocation == ~stale
=============================================================================
