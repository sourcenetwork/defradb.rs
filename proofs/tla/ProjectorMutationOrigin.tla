--------------------- MODULE ProjectorMutationOrigin ---------------------
\* A request may enable a capability only with its own native authority.
\* A failed source change leaves both the document and grant unchanged.
\* Revocation before commit conflicts with the authorization observation.
EXTENDS Naturals
\* Bug selects one deliberately broken guard for each red model.
CONSTANT Bug
\* The source and document are committed together; the actor is fixed at start.
VARIABLES manager, pending, actor, observed, document, grant, accepted, valid
\* All variables belong to the state checked by TLC.
vars == <<manager, pending, actor, observed, document, grant, accepted, valid>>
\* Begin with an authorized manager and no published configuration or grant.
Init == /\ manager = TRUE /\ pending = FALSE /\ actor = "none"
        /\ observed = FALSE /\ document = FALSE /\ grant = FALSE
        /\ accepted = FALSE /\ valid = TRUE
\* A request or an explicitly invoked Node-owned repair starts from one snapshot.
Start(a) == /\ ~pending /\ ~accepted
            /\ pending' = TRUE /\ actor' = a /\ observed' = manager
            /\ UNCHANGED <<manager, document, grant, accepted, valid>>
\* An independent transaction may revoke the request actor's authority.
Revoke == /\ manager /\ manager' = FALSE
          /\ UNCHANGED <<pending, actor, observed, document, grant, accepted, valid>>
\* The repair actor is the authenticated Node, not a substitute for a writer.
\* Successful enablement publishes the document and source together.
Commit == /\ pending
          /\ (actor = "repair" \/ (actor = "admin" /\ observed /\ manager)
              \/ (Bug = "Actor" /\ observed)
              \/ (Bug = "Revocation" /\ actor = "admin" /\ observed))
          /\ pending' = FALSE /\ document' = TRUE /\ grant' = TRUE
          /\ accepted' = TRUE
          /\ valid' = (actor = "repair" \/ (actor = "admin" /\ manager))
          /\ UNCHANGED <<manager, actor, observed>>
\* A failed statement rolls back both halves.
Abort == /\ pending /\ pending' = FALSE
         /\ document' = IF Bug = "Atomic" THEN TRUE ELSE document
         /\ UNCHANGED <<manager, actor, observed, grant, accepted, valid>>
\* Explore callers without manager rights and calls without an actor as well.
Next == (\E a \in {"admin", "writer", "absent", "repair"}: Start(a))
        \/ Revoke \/ Commit \/ Abort
\* Steps may also stutter, as real transactions can wait.
Spec == Init /\ [][Next]_vars
\* A published grant was authorized by its actual origin at commit.
INV_Authority == ~accepted \/ valid
\* The required grant and configuration never publish separately.
INV_Atomic == document = grant
=============================================================================
