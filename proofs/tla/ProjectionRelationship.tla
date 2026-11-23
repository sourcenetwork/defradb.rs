------------------------- MODULE ProjectionRelationship -------------------------
\* An actor projects a relationship while another transaction may revoke its authority.
\* Native authorization, registration and publication share the same transaction.
\* What must never happen: a relationship published while any required fact is
\* missing, after a revocation committed first, or by an aborted transaction.
\* The "document" fact is waived for reconciliation: startup reconciliation and
\* project_related may change any document, still as the request's actor (or
\* the node) and under every other guard.
\* The code: DocumentMutationContext::change_projected_relationship in
\* crates/database/db/src/document_projection.rs, with acp::projection::in_scope
\* (crates/engine/acp/src/projection.rs) and, for an external provider,
\* TransactionOutbox::stage_prepared (crates/engine/acp/src/outbox/staging.rs).
\* Only the standard operators on numbers are used.
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
