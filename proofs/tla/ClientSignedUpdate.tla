---- MODULE ClientSignedUpdate ----
\* One client approves an update from one snapshot. Another writer and an
\* ACP administrator can change the database while signatures are in flight.
\* The update may commit only if its transaction still observes the approved
\* written field and permission and both reads survive commit validation. A cancelled
\* client releases no more signatures. A disjoint field can change concurrently.
\* Real field versions and signed bytes are abstract atoms. Exact intent
\* excludes writes to immutable fields and checks their genesis signature.
\* Rust owners: db client_update.rs and client_waves.rs, HTTP signing,
\* and the transactional update and ACP reads.
EXTENDS Naturals
\* RED models remove exactly one guard.
CONSTANT CheckIntent, CheckLive, ValidateField, ValidateAcp
\* The current database and the transaction's snapshot are represented by bits.
VARIABLES field, other, allowed, live, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp
\* These are the complete state explored by TLC.
vars == <<field, other, allowed, live, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp>>
\* The user approves the field at version zero while permission and the conversation are live.
Init == /\ field = 0 /\ other = 0 /\ allowed = TRUE /\ live = TRUE /\ phase = "offer"
        /\ seenField = 0 /\ seenAcp = TRUE
        /\ badIntent = FALSE /\ badLive = FALSE /\ badField = FALSE /\ badAcp = FALSE
\* A different writer changes the field this update writes.
ChangeField == /\ field = 0 /\ field' = 1
              /\ UNCHANGED <<other, allowed, live, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp>>
\* A disjoint field may change while the update's transaction is active.
ChangeOther == /\ other = 0 /\ phase = "attempt" /\ other' = 1
               /\ UNCHANGED <<field, allowed, live, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp>>
\* An administrator revokes the actor's permission.
Revoke == /\ allowed /\ allowed' = FALSE
          /\ UNCHANGED <<other, field, live, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp>>
\* The user can cancel before the transaction starts.
Cancel == /\ live /\ live' = FALSE
          /\ UNCHANGED <<other, field, allowed, phase, seenField, seenAcp, badIntent, badLive, badField, badAcp>>
\* Only the approved exact graph receives a signature from a live client.
Sign(exact) == /\ phase = "offer" /\ (CheckIntent => exact) /\ (CheckLive => live)
               /\ phase' = "signed" /\ badIntent' = (badIntent \/ ~exact)
               /\ badLive' = (badLive \/ ~live)
               /\ UNCHANGED <<other, field, allowed, live, seenField, seenAcp, badField, badAcp>>
\* An attempt takes its relevant field and permission decisions in the same snapshot.
Begin == /\ phase = "signed" /\ field = 0 /\ allowed /\ live
         /\ seenField' = field /\ seenAcp' = allowed /\ phase' = "attempt"
         /\ UNCHANGED <<other, field, allowed, live, badIntent, badLive, badField, badAcp>>
\* Validation prevents committing a stale written field or revoked permission.
Commit == /\ phase = "attempt" /\ (ValidateField => seenField = field)
          /\ (ValidateAcp => seenAcp = allowed)
          /\ phase' = "done" /\ badField' = (badField \/ (field # 0))
          /\ badAcp' = (badAcp \/ ~allowed)
          /\ UNCHANGED <<other, field, allowed, live, seenField, seenAcp, badIntent, badLive>>
\* The database, client and administrator may act in any order.
Next == ChangeField \/ ChangeOther \/ Revoke \/ Cancel \/ Begin \/ Commit \/ (\E exact \in BOOLEAN : Sign(exact))
\* Stuttering models unavailable actors.
Spec == Init /\ [][Next]_vars
\* Signatures are never released for an unapproved graph.
INV_Intent == ~badIntent
\* An ended conversation releases no signature.
INV_Live == ~badLive
\* A commit never crosses a changed written field.
INV_Field == ~badField
\* This deliberately false claim exposes a disjoint write that may commit.
INV_NoDisjointCommit == ~(phase = "done" /\ other = 1)
\* A commit never crosses a revocation.
INV_Acp == ~badAcp
====
