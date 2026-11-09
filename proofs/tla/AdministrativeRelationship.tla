------------------------ MODULE AdministrativeRelationship ------------------------
\* A foreign User owns a document replicated onto a Node. A Node administrator
\* may grant or revoke a declared relation without impersonating its owner.
\* Active NAC must authorize the real actor for bypass and the specific DAC
\* relation operation; NAC, registration, DAC and publication share one native
\* transaction. A concurrent NAC revocation must invalidate its commit. Only a
\* trusted embedder's route may use this authority: Go's public routes keep its
\* rule that only a document's managers change its relationships.
\* Rust owners: acp DocumentACP, ZanzibarDocumentACP administrative relation,
\* and defra-node acp_ops/transactional.rs relationship_in_txn.
EXTENDS Naturals
\* Each RED configuration removes one named guard.
CONSTANT Bug, Scenario
\* The finite cases cover real authority, every denial, and a concurrent revoke.
Cases == {"good", "publicRoute", "actor", "active", "nacBound", "bypass", "operation", "dacBound", "registered", "relation", "revoke", "rollback"}
\* Track the transaction, whether its NAC read is current, and its visible effect.
VARIABLES scenario, phase, nacLive, visible
\* Every step describes all of the mutable state.
vars == <<scenario, phase, nacLive, visible>>
\* No relation is visible before the transaction.
Init == /\ scenario \in (IF Scenario = "all" THEN Cases ELSE {Scenario})
        /\ phase = "ready" /\ nacLive = TRUE /\ visible = FALSE
\* The claimed effect actor equals the authenticated actual actor.
Actor == scenario # "actor"
\* Disabled NAC never produces a bypass capability.
Active == scenario # "active"
\* The active node authority is read in this transaction.
NacBound == scenario # "nacBound"
\* The actual actor holds native bypass-dac.
Bypass == scenario # "bypass"
\* The actual actor holds the specific native add or delete permission.
Operation == scenario # "operation"
\* The native DAC provider shares this transaction.
DacBound == scenario # "dacBound"
\* The foreign document has been registered before its relation is changed.
Registered == scenario # "registered"
\* The policy relation and its subject are valid; owner remains immutable.
Relation == scenario # "relation" /\ scenario # "rollback"
\* The change arrived through a trusted embedder's route, not a public one.
EmbedderRoute == scenario # "publicRoute"
\* Every condition is required before an administrator effect can be staged.
Allowed == /\ (Actor \/ Bug = "actor") /\ (Active \/ Bug = "active")
           /\ (NacBound \/ Bug = "nacBound") /\ (Bypass \/ Bug = "bypass")
           /\ (Operation \/ Bug = "operation") /\ (DacBound \/ Bug = "dacBound")
           /\ (Registered \/ Bug = "registered") /\ (Relation \/ Bug = "relation")
           /\ (EmbedderRoute \/ Bug = "route")
\* A rejected request must not make the relation visible.
Stage == /\ phase = "ready" /\ phase' = IF Allowed THEN "staged" ELSE "aborted"
         /\ visible' = (~Allowed /\ Bug = "rollback")
         /\ UNCHANGED <<scenario, nacLive>>
\* The node administrator revokes NAC after the transaction read.
Revoke == /\ phase = "staged" /\ scenario = "revoke" /\ nacLive
          /\ nacLive' = FALSE /\ UNCHANGED <<scenario, phase, visible>>
\* Commit validates the NAC read; conflict rolls back the relation.
Commit == /\ phase = "staged"
          /\ visible' = (nacLive \/ Bug = "revoke")
          /\ phase' = IF visible' THEN "done" ELSE "aborted"
          /\ UNCHANGED <<scenario, nacLive>>
\* The transaction may also be explicitly discarded.
Discard == /\ phase = "staged" /\ phase' = "aborted"
           /\ UNCHANGED <<scenario, nacLive, visible>>
\* The owner and administrator can interleave their steps.
Next == Stage \/ Revoke \/ Commit \/ Discard
\* Stuttering permits the transaction or administrator to wait.
Spec == Init /\ [][Next]_vars
\* A published relation always uses the authenticated actor.
INV_Actor == visible => Actor
\* Disabled NAC never grants administrative authority.
INV_Active == visible => Active
\* The NAC read and relation write share a transaction.
INV_NacBound == visible => NacBound
\* A published administrative relation requires bypass-dac.
INV_Bypass == visible => Bypass
\* A published administrative relation requires its add/delete permission.
INV_Operation == visible => Operation
\* The DAC provider is bound to the same transaction.
INV_DacBound == visible => DacBound
\* A published relation belongs to an already registered document.
INV_Registered == visible => Registered
\* A published relation is declared and never owner.
INV_Relation == visible => Relation
\* A published administrative relation came through a trusted embedder's route.
INV_EmbedderRoute == visible => EmbedderRoute
\* Revoking the NAC authority before commit prevents the write.
INV_Revocation == visible => nacLive
\* A rejected statement leaves no visible relation.
INV_Rollback == phase = "aborted" => ~visible
=============================================================================
