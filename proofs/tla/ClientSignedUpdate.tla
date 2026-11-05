---- MODULE ClientSignedUpdate ----
\* One client approves an update from one snapshot. Another writer and an
\* ACP administrator can change the database while signatures are in flight.
\* The update may commit only if its transaction still observes the approved
\* written field and permission and both reads survive commit validation. A cancelled
\* client releases no more signatures. A disjoint field can change concurrently.
\* Real field versions and signed bytes are abstract atoms.
\* Planned Rust owners: db block builder, HTTP signing and transaction reads.
EXTENDS Naturals
\* RED models remove exactly one guard.
CONSTANT CheckIntent, CheckLive, ValidateHeads, ValidateAcp
\* The current database and the transaction's snapshot are represented by bits.
VARIABLES head, other, allowed, live, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp
\* These are the complete state explored by TLC.
vars == <<head, other, allowed, live, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp>>
\* The user approves head zero while permission and the conversation are live.
Init == /\ head = 0 /\ other = 0 /\ allowed = TRUE /\ live = TRUE /\ phase = "offer"
        /\ seenHead = 0 /\ seenAcp = TRUE
        /\ badIntent = FALSE /\ badLive = FALSE /\ badHead = FALSE /\ badAcp = FALSE
\* A different writer changes the field this update writes.
ChangeHead == /\ head = 0 /\ head' = 1
              /\ UNCHANGED <<other, allowed, live, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp>>
\* A disjoint field may change while the update's transaction is active.
ChangeOther == /\ other = 0 /\ phase = "attempt" /\ other' = 1
               /\ UNCHANGED <<head, allowed, live, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp>>
\* An administrator revokes the actor's permission.
Revoke == /\ allowed /\ allowed' = FALSE
          /\ UNCHANGED <<other, head, live, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp>>
\* The user can cancel before the transaction starts.
Cancel == /\ live /\ live' = FALSE
          /\ UNCHANGED <<other, head, allowed, phase, seenHead, seenAcp, badIntent, badLive, badHead, badAcp>>
\* Only the approved exact graph receives a signature from a live client.
Sign(exact) == /\ phase = "offer" /\ (CheckIntent => exact) /\ (CheckLive => live)
               /\ phase' = "signed" /\ badIntent' = (badIntent \/ ~exact)
               /\ badLive' = (badLive \/ ~live)
               /\ UNCHANGED <<other, head, allowed, live, seenHead, seenAcp, badHead, badAcp>>
\* An attempt takes its relevant field and permission decisions in the same snapshot.
Begin == /\ phase = "signed" /\ head = 0 /\ allowed /\ live
         /\ seenHead' = head /\ seenAcp' = allowed /\ phase' = "attempt"
         /\ UNCHANGED <<other, head, allowed, live, badIntent, badLive, badHead, badAcp>>
\* Validation prevents committing a stale written field or revoked permission.
Commit == /\ phase = "attempt" /\ (ValidateHeads => seenHead = head)
          /\ (ValidateAcp => seenAcp = allowed)
          /\ phase' = "done" /\ badHead' = (badHead \/ (head # 0))
          /\ badAcp' = (badAcp \/ ~allowed)
          /\ UNCHANGED <<other, head, allowed, live, seenHead, seenAcp, badIntent, badLive>>
\* The database, client and administrator may act in any order.
Next == ChangeHead \/ ChangeOther \/ Revoke \/ Cancel \/ Begin \/ Commit \/ (\E exact \in BOOLEAN : Sign(exact))
\* Stuttering models unavailable actors.
Spec == Init /\ [][Next]_vars
\* Signatures are never released for an unapproved graph.
INV_Intent == ~badIntent
\* An ended conversation releases no signature.
INV_Live == ~badLive
\* A commit never crosses a changed written field.
INV_Heads == ~badHead
\* This deliberately false claim exposes a disjoint write that may commit.
INV_NoDisjointCommit == ~(phase = "done" /\ other = 1)
\* A commit never crosses a revocation.
INV_Acp == ~badAcp
====
