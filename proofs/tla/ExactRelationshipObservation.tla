-------------------- MODULE ExactRelationshipObservation --------------------
\* The story. A node answers whether one actor holds one relation on a document
\* by reading three certified proofs from Vera, one after another: the policy,
\* the object's incarnation and the relationship record. Vera certifies a new
\* root at any time, so each read may see a different root.
\* What must never happen: an answer assembled from proofs of different roots,
\* or an observation that fails while it could still be asked again: a root
\* certified between two reads is a late answer, not a failure.
\* The code: exact_relationship::query in
\* crates/engine/vera/src/vera_rs/provider/exact_relationship.rs, which compares
\* every proof's module_state_root with the policy's (at_policy_root): a
\* mismatch is ExactRelationshipError::Overtaken, and the provider asks the
\* whole observation again through verified (provider/verified.rs).
\* Only the standard operators on numbers are used.
EXTENDS Naturals
\* Bug accepts an answer whatever roots its proofs name. Retry asks the whole
\* observation again, from the policy read, when its proofs name different
\* roots, at most Attempts asks in all. MaxAdvances bounds how often a new
\* root is certified, to keep the model small.
CONSTANTS Bug, Retry, Attempts, MaxAdvances
\* root: the latest certified root. policyRoot, objectRoot, relationRoot: the
\* root each proof of the current ask was read at. phase: the next read.
\* asked: how many times the observation has been asked. answer: the outcome.
VARIABLES root, policyRoot, objectRoot, relationRoot, phase, asked, answer
\* Every variable, for stuttering steps.
vars == <<root, policyRoot, objectRoot, relationRoot, phase, asked, answer>>
\* The first ask starts before any proof is collected.
Init == /\ root = 0 /\ policyRoot = "unset" /\ objectRoot = "unset"
        /\ relationRoot = "unset" /\ phase = "policy" /\ asked = 1 /\ answer = "none"
\* A new certified root may appear between any two reads: the head advances.
Advance == /\ root < MaxAdvances /\ root' = root + 1
           /\ UNCHANGED <<policyRoot, objectRoot, relationRoot, phase, asked, answer>>
\* Each proof is bound to the root observed for its own read.
ReadPolicy == /\ phase = "policy" /\ policyRoot' = root /\ phase' = "object"
              /\ UNCHANGED <<root, objectRoot, relationRoot, asked, answer>>
\* The object incarnation is read next, at the root current then.
ReadObject == /\ phase = "object" /\ objectRoot' = root /\ phase' = "relation"
              /\ UNCHANGED <<root, policyRoot, relationRoot, asked, answer>>
\* The relationship record is read last, at the root current then.
ReadRelation == /\ phase = "relation" /\ relationRoot' = root /\ phase' = "decide"
                /\ UNCHANGED <<root, policyRoot, objectRoot, asked, answer>>
\* The proofs of this ask name one root.
Coherent == policyRoot = objectRoot /\ objectRoot = relationRoot
\* Proofs at one root answer. Proofs at different roots are an overtaken
\* ask: asked again from the policy read while asks are left, refused once
\* they are spent.
Decide == /\ phase = "decide"
          /\ IF Bug \/ Coherent
                THEN /\ answer' = "result" /\ phase' = "done" /\ UNCHANGED asked
                ELSE IF Retry /\ asked < Attempts
                     THEN /\ phase' = "policy" /\ asked' = asked + 1
                          /\ UNCHANGED answer
                     ELSE /\ answer' = "refused" /\ phase' = "done" /\ UNCHANGED asked
          /\ UNCHANGED <<root, policyRoot, objectRoot, relationRoot>>
\* The observation is over.
Done == /\ phase = "done" /\ UNCHANGED vars
\* Reads and new roots interleave in any order.
Next == Advance \/ ReadPolicy \/ ReadObject \/ ReadRelation \/ Decide \/ Done
\* Steps may also wait.
Spec == Init /\ [][Next]_vars
\* An accepted answer is backed by one coherent certified state.
INV_SameRoot == answer = "result" => Coherent
\* An observation fails only once every one of its asks was overtaken.
INV_FailsOnlyWhenEveryAskOvertaken == answer = "refused" => asked = Attempts
=============================================================================
