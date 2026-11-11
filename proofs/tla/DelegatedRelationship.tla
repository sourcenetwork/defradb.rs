------------------------ MODULE DelegatedRelationship ------------------------
\* A trusted embedding process owns a transaction and authenticates the actor
\* whose relationship it changes. The registry checks the transaction owner;
\* NAC and DAC check that actor, not the node owning the transaction. HTTP
\* cannot invoke this privileged embedding operation.
EXTENDS Naturals
CONSTANT Bug                            \* Each RED config removes one guard.
VARIABLES done, violation               \* Whether a request was unsafe.
vars == <<done, violation>>
Init == /\ done = FALSE                \* No request has completed.
        /\ violation = FALSE           \* No unsafe write was staged.

Request(trusted, owner, authenticated, nac, dac, bound) ==
  /\ ~done                             \* Explore one bounded request.
  /\ LET allowed ==                    \* Each check must hold for this actor.
       (trusted \/ Bug = "Http") /\
       (owner \/ Bug = "Owner") /\
       (authenticated \/ Bug = "Actor") /\
       (nac \/ Bug = "Nac") /\
       (dac \/ Bug = "Dac") /\
       (bound \/ Bug = "Bound")
     IN /\ done' = allowed            \* Denied requests make no change.
        /\ violation' = (allowed /\ ~(trusted /\ owner /\ authenticated /\ nac /\ dac /\ bound))

Next == \E trusted, owner, authenticated, nac, dac, bound \in BOOLEAN :
  Request(trusted, owner, authenticated, nac, dac, bound)
Spec == Init /\ [][Next]_vars          \* Explore every possible input.
INV_ActorAuthority == ~violation       \* No write escapes a required check.
=============================================================================
