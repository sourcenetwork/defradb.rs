-------------------- MODULE ExactRelationshipObservation --------------------
\* The story. A node answers whether one actor holds one relation on a document
\* by reading three certified proofs from Vera, one after another: the policy,
\* the object's incarnation and the relationship record. Vera certifies a new
\* root at any time, so each read may see a different root.
\* What must never happen: an answer assembled from proofs of different roots.
\* The code: exact_relationship::query in
\* crates/engine/vera/src/vera_rs/provider/exact_relationship.rs, which compares
\* every proof's module_state_root with the policy's (at_policy_root) and refuses a mismatch.
\* Only the standard operators on numbers are used.
EXTENDS Naturals
\* Bug accepts an answer whatever roots its proofs name.
CONSTANT Bug
\* root: the latest certified root. policyRoot, objectRoot, relationRoot: the
\* root each proof was read at. phase: the next read. answer: the outcome.
VARIABLES root, policyRoot, objectRoot, relationRoot, phase, answer
\* Every variable, for stuttering steps.
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
\* The object incarnation is read next, at the root current then.
ReadObject == /\ phase = "object" /\ objectRoot' = root /\ phase' = "relation"
              /\ UNCHANGED <<root, policyRoot, relationRoot, answer>>
\* The relationship record is read last, at the root current then.
ReadRelation == /\ phase = "relation" /\ relationRoot' = root /\ phase' = "decide"
                /\ UNCHANGED <<root, policyRoot, objectRoot, answer>>
\* Only proofs at one root may produce a definitive answer.
Decide == /\ phase = "decide"
          /\ answer' = IF Bug \/ (policyRoot = objectRoot /\ objectRoot = relationRoot)
                        THEN "result" ELSE "refused"
          /\ phase' = "done"
          /\ UNCHANGED <<root, policyRoot, objectRoot, relationRoot>>
\* The read is over.
Done == /\ phase = "done" /\ UNCHANGED vars
\* Reads and new roots interleave in any order.
Next == Advance \/ ReadPolicy \/ ReadObject \/ ReadRelation \/ Decide \/ Done
\* Steps may also wait.
Spec == Init /\ [][Next]_vars
\* An accepted answer is backed by one coherent certified state.
INV_SameRoot == answer = "result" => policyRoot = objectRoot /\ objectRoot = relationRoot
=============================================================================
