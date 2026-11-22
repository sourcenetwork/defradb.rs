---- MODULE MissingRelationFilter ----
\* A missing child has null fields. Scalar evaluation supplies idMatches and
\* siblingsMatch; this model checks the join's admission rule, not the scalar
\* evaluator or the document mapping.
EXTENDS Naturals
\* HasId records the explicit ID condition; the match flags come from scalar evaluation.
\* Mode selects the admission rule or one of the two broken variants.
CONSTANTS HasId, IdMatches, SiblingsMatch, Mode
\* Only boolean inputs and known rules are modeled.
ASSUME /\ HasId \in BOOLEAN /\ IdMatches \in BOOLEAN
       /\ SiblingsMatch \in BOOLEAN
       /\ Mode \in {"correct", "noIdGuard", "ignoreSibling"}
\* decided records whether the join has evaluated; accepted records its answer.
VARIABLES decided, accepted
\* These are all variables that may change.
vars == <<decided, accepted>>
\* Before evaluation the join has not accepted a child.
Init == /\ decided = FALSE /\ accepted = FALSE
\* Evaluate once: require the ID condition and both scalar results.
\* The broken modes omit the explicit ID guard or the sibling result.
Decide ==
  /\ ~decided
  /\ accepted' = CASE Mode = "noIdGuard" -> IdMatches /\ SiblingsMatch
                  [] Mode = "ignoreSibling" -> HasId /\ IdMatches
                  [] OTHER -> HasId /\ IdMatches /\ SiblingsMatch
  /\ decided' = TRUE
\* Evaluation is the only modeled action.
Next == Decide
\* The system starts at Init and may evaluate or stutter.
Spec == Init /\ [][Next]_vars
\* The answer and evaluation flag stay boolean.
TypeOK == /\ decided \in BOOLEAN /\ accepted \in BOOLEAN
\* An accepted missing child must satisfy every required condition.
Safe == accepted => (HasId /\ IdMatches /\ SiblingsMatch)
\* Once evaluated, acceptance is exactly the required conjunction.
Exact == decided => (accepted = (HasId /\ IdMatches /\ SiblingsMatch))
====
