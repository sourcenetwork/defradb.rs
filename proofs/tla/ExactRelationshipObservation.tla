-------------------- MODULE ExactRelationshipObservation --------------------
\* An observation must not combine proofs from different certified roots.
EXTENDS Naturals
CONSTANT Bug
VARIABLES root, policyRoot, objectRoot, relationRoot, phase, answer
vars == <<root, policyRoot, objectRoot, relationRoot, phase, answer>>
\* The read starts before any proof is collected.
Init == /\ root = 0 /\ policyRoot = "unset" /\ objectRoot = "unset"
        /\ relationRoot = "unset" /\ phase = "policy" /\ answer = "none"
\* A new certified root may appear between any two reads.
Advance == /\ root < 2 /\ root' = root + 1
           /\ UNCHANGED <<policyRoot, objectRoot, relationRoot, phase, answer>>
\* Each proof is bound to the root observed for its own read.
ReadPolicy == /\ phase = "policy" /\ policyRoot' = root /\ phase' = "object"
              /\ UNCHANGED <<root, objectRoot, relationRoot, answer>>
ReadObject == /\ phase = "object" /\ objectRoot' = root /\ phase' = "relation"
              /\ UNCHANGED <<root, policyRoot, relationRoot, answer>>
ReadRelation == /\ phase = "relation" /\ relationRoot' = root /\ phase' = "decide"
                /\ UNCHANGED <<root, policyRoot, objectRoot, answer>>
\* Only proofs at one root may produce a definitive answer.
Decide == /\ phase = "decide"
          /\ answer' = IF Bug \/ (policyRoot = objectRoot /\ objectRoot = relationRoot)
                        THEN "result" ELSE "refused"
          /\ phase' = "done"
          /\ UNCHANGED <<root, policyRoot, objectRoot, relationRoot>>
Done == /\ phase = "done" /\ UNCHANGED vars
Next == Advance \/ ReadPolicy \/ ReadObject \/ ReadRelation \/ Decide \/ Done
Spec == Init /\ [][Next]_vars
\* An accepted answer is backed by one coherent certified state.
INV_SameRoot == answer = "result" => policyRoot = objectRoot /\ objectRoot = relationRoot
=============================================================================
