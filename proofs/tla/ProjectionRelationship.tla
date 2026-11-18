------------------------- MODULE ProjectionRelationship -------------------------
\* An actor projects a relationship while another transaction may revoke its authority.
\* Native authorization, registration and publication share the same transaction.
EXTENDS Naturals
\* Bug removes one guard; Scenario selects which authority or scope is absent.
CONSTANTS Bug, Scenario
\* The transaction can stage, commit, or abort; authority can be revoked.
VARIABLES phase, live, visible
\* Every transition accounts for all state.
vars == <<phase, live, visible>>
\* No projection is published before the transaction begins.
Init == /\ phase = "ready" /\ live = TRUE /\ visible = FALSE
\* Each named scenario denies one required native fact.
Facts == {"context", "actor", "active", "nacBound", "operation", "dacBound", "dacAllowed", "document", "scope", "registered", "relation"}
\* Every missing fact must prevent staging unless its guard is deliberately broken.
Allowed == \A fact \in Facts : Scenario # fact \/ Bug = fact
\* A denied transaction is aborted without publishing a relation.
Stage == /\ phase = "ready" /\ phase' = IF Allowed THEN "staged" ELSE "aborted"
         /\ UNCHANGED <<live, visible>>
\* A concurrent transaction can revoke the native operation permission.
Revoke == /\ phase = "staged" /\ Scenario = "revoke" /\ live
          /\ live' = FALSE /\ UNCHANGED <<phase, visible>>
\* The transaction validates its authorization read before publishing.
Commit == /\ phase = "staged" /\ visible' = (live \/ Bug = "revoke")
          /\ phase' = (IF visible' THEN "done" ELSE "aborted") /\ UNCHANGED live
\* An abort discards staged changes.
Discard == /\ phase = "staged" /\ phase' = "aborted" /\ UNCHANGED <<live, visible>>
\* Any enabled transition may run, interleaved with the revocation.
Next == Stage \/ Revoke \/ Commit \/ Discard
\* Stuttering allows both transactions to wait.
Spec == Init /\ [][Next]_vars
\* Published relationships satisfy every context and scope guard.
INV_Guards == visible => Scenario \notin Facts
\* A revocation that commits first prevents publication.
INV_Revocation == visible => live
\* An aborted transaction publishes nothing.
INV_Rollback == phase = "aborted" => ~visible
=============================================================================
