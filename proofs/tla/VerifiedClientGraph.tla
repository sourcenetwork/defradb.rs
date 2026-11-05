-------------------------- MODULE VerifiedClientGraph --------------------------
\* A User supplies signed public evidence for one create, update, or delete.
\* A managed Agent has already staged an effect in the Node-owned transaction.
\* The node checks the same operation it will execute, the signer and exact
\* graph, and active transaction-bound NAC and DAC. Another transaction can
\* revoke either authority before commit. A rejected or conflicted handoff
\* discards both the User operation and the Agent effect.
\*
\* Blocks, keys, signatures and physical heads are abstract facts. The model
\* assumes native cryptographic verification and Regolith's snapshot/read-set
\* validation; it does not prove their encodings or algorithms. In particular,
\* an inactive authority imposes no permission or transaction-binding check.
\*
\* Rust owners: crates/db/src/block/builder/verified_client_{create,mutation}.rs,
\* crates/defra-node/src/lib.rs execute_verified_graph_in_txn,
\* crates/query/src/runner/turn.rs lent, and the native transaction commit.
EXTENDS Naturals

\* Each RED configuration removes exactly one guard.
CONSTANT Bug, Scenario

\* The scenarios cover the valid path, inactive ACP and each rejected fact.
Cases == {"good", "inactive", "parse", "rollback", "actor", "signature", "target",
          "heads", "rehead", "wave", "schema", "bypass", "owner", "nac", "dac", "nacBound", "dacBound", "revokeNac", "revokeDac"}

\* All three supported mutation kinds share the same transaction owner.
Operations == {"create", "update", "delete"}

\* The phase and visible effects describe the same single transaction.
VARIABLES operation, scenario, phase, headLive, nacLive, dacLive, agentVisible, userVisible
\* The tuple gives TLC every variable it must preserve on a step.
vars == <<operation, scenario, phase, headLive, nacLive, dacLive, agentVisible, userVisible>>

\* No effect is visible before the transaction starts; both permissions are live.
Init == /\ operation \in Operations
        /\ scenario \in (IF Scenario = "all" THEN Cases ELSE {Scenario})
        /\ phase = "ready"
        /\ headLive = TRUE /\ nacLive = TRUE /\ dacLive = TRUE
        /\ agentVisible = FALSE /\ userVisible = FALSE

\* A second typed operation would override the parsed text; a failure scenario
\* also uses this rejected input to exercise whole-transaction rollback.
Canonical == scenario # "parse" /\ scenario # "rollback"
\* The effect actor must be exactly the signer derived from the verified key.
ActorMatches == scenario # "actor"
\* The signed public blocks must verify under that key.
SignatureValid == scenario # "signature"
\* The selected physical target and input must match the verified graph.
TargetMatches == scenario # "target"
\* Updates and deletes must produce the approved bytes from the current heads.
HeadsMatch == operation = "create" \/ scenario # "heads"
\* This execution must consume every approved signed block, not just return a row.
WaveConsumed == scenario # "wave"
\* The schema used by this execution must be the transaction's schema.
SchemaBound == scenario # "schema"
\* A request cannot carry DAC bypass into a verified graph execution.
NoBypass == scenario # "bypass"
\* The transaction handle belongs to the Node that executes this request.
NodeOwnsTransaction == scenario # "owner"
\* The inactive scenario deliberately has neither NAC nor document policy.
NacActive == scenario # "inactive"
\* The document policy is inactive in the same deliberately unbound collection.
DacActive == scenario # "inactive"
\* An active NAC checks the User's own permission and has a bound provider.
NacAllowed == scenario # "nac"
\* An active DAC checks the User's own permission and has a bound provider.
DacAllowed == scenario # "dac"
\* An active NAC decision must share the transaction snapshot.
NacBound == scenario # "nacBound"
\* An active DAC decision must share the transaction snapshot.
DacBound == scenario # "dacBound"

\* The exact operation, signature and active authorization guards all apply.
Admitted ==
  /\ (Canonical \/ Bug = "parse")
  /\ (ActorMatches \/ Bug = "actor")
  /\ (SignatureValid \/ Bug = "signature")
  /\ (TargetMatches \/ Bug = "target")
  /\ (HeadsMatch \/ Bug = "heads")
  /\ (WaveConsumed \/ Bug = "wave")
  /\ (SchemaBound \/ Bug = "schema")
  /\ (NoBypass \/ Bug = "bypass")
  /\ (NodeOwnsTransaction \/ Bug = "owner")
  /\ (~NacActive \/ NacAllowed \/ Bug = "nac")
  /\ (~DacActive \/ DacAllowed \/ Bug = "dac")
  /\ (~NacActive \/ NacBound \/ Bug = "nacBound")
  /\ (~DacActive \/ DacBound \/ Bug = "dacBound")

\* The Agent effect is only staged; an invalid User graph discards it.
Stage == /\ phase = "ready"
         /\ phase' = IF Admitted THEN "staged" ELSE "aborted"
         /\ agentVisible' = (~Admitted /\ Bug = "rollback")
         /\ UNCHANGED <<operation, scenario, headLive, nacLive, dacLive, userVisible>>

\* Another writer can change the selected head after the signed transaction reads it.
ChangeHead == /\ phase = "staged" /\ scenario = "rehead" /\ headLive
              /\ headLive' = FALSE
              /\ UNCHANGED <<operation, scenario, phase, nacLive, dacLive, agentVisible, userVisible>>

\* A node administrator can revoke NAC after staging but before commit.
RevokeNac == /\ phase = "staged" /\ scenario = "revokeNac" /\ nacLive
             /\ nacLive' = FALSE
             /\ UNCHANGED <<operation, scenario, phase, headLive, dacLive, agentVisible, userVisible>>

\* A document administrator can independently revoke DAC at the same point.
RevokeDac == /\ phase = "staged" /\ scenario = "revokeDac" /\ dacLive
             /\ dacLive' = FALSE
             /\ UNCHANGED <<operation, scenario, phase, headLive, nacLive, agentVisible, userVisible>>

\* Commit validates the transaction's relevant authorization reads. A failed
\* validation discards both staged effects; success publishes both together.
Commit == /\ phase = "staged"
          /\ LET valid == (headLive \/ Bug = "rehead") /\ (nacLive \/ Bug = "revokeNac") /\ (dacLive \/ Bug = "revokeDac")
             IN /\ phase' = IF valid THEN "done" ELSE "aborted"
                /\ agentVisible' = valid
                /\ userVisible' = valid
          /\ UNCHANGED <<operation, scenario, headLive, nacLive, dacLive>>

\* A caller may abandon the staged transaction without publishing either effect.
Discard == /\ phase = "staged" /\ phase' = "aborted"
           /\ UNCHANGED <<operation, scenario, headLive, nacLive, dacLive, agentVisible, userVisible>>

\* The client, transaction and either administrator can interleave.
Next == Stage \/ ChangeHead \/ RevokeNac \/ RevokeDac \/ Commit \/ Discard
\* Stuttering allows the transaction or the administrator to wait.
Spec == Init /\ [][Next]_vars

\* A published User effect used the same canonical operation that was checked.
INV_Canonical == userVisible => Canonical
\* A published User effect has the verified signer's identity.
INV_Actor == userVisible => ActorMatches
\* A published User effect has valid User signatures.
INV_Signature == userVisible => SignatureValid
\* A published User effect has the exact physical target and input.
INV_Target == userVisible => TargetMatches
\* A published update or delete used the approved current heads.
INV_Heads == userVisible => HeadsMatch
\* Commit validates the head read even when another writer changes it later.
INV_HeadConflict == userVisible => headLive
\* A published effect consumed the full verified signing wave.
INV_Wave == userVisible => WaveConsumed
\* A published effect used the transaction's schema.
INV_Schema == userVisible => SchemaBound
\* A published effect did not bypass DAC.
INV_Bypass == userVisible => NoBypass
\* A published effect belongs to the Node's transaction.
INV_Owner == userVisible => NodeOwnsTransaction
\* An active NAC authorized the User.
INV_Nac == userVisible => (~NacActive \/ NacAllowed)
\* An active DAC authorized the User.
INV_Dac == userVisible => (~DacActive \/ DacAllowed)
\* An active NAC decision was transaction-bound.
INV_NacBound == userVisible => (~NacActive \/ NacBound)
\* An active DAC decision was transaction-bound.
INV_DacBound == userVisible => (~DacActive \/ DacBound)
\* A concurrent NAC revoke cannot be followed by a stale commit.
INV_NacRevocation == userVisible => nacLive
\* A concurrent DAC revoke cannot be followed by a stale commit.
INV_DacRevocation == userVisible => dacLive
\* The User mutation and the previously staged Agent effect commit together.
INV_Atomic == userVisible = agentVisible
=============================================================================
