---- MODULE MissingRelationFilter ----
\* A missing child has null fields. The scalar evaluator supplies the ID and
\* sibling results; the join may accept only when an explicit ID and both
\* results are present. Each run checks every combination of those inputs.
EXTENDS Naturals  \* Use the standard Boolean values.
CONSTANT Mode  \* Select the correct rule or a broken rule.
ASSUME Mode \in {"correct", "noIdGuard", "ignoreSibling"}  \* Refuse unknown rules.
VARIABLES hasId, idMatches, siblingsMatch, decided, accepted  \* Hold inputs and the decision.
vars == <<hasId, idMatches, siblingsMatch, decided, accepted>>  \* List the complete state.
Init == /\ hasId \in BOOLEAN /\ idMatches \in BOOLEAN /\ siblingsMatch \in BOOLEAN  \* Pick any input combination.
        /\ decided = FALSE /\ accepted = FALSE  \* No decision has been made.
Decide == /\ ~decided  \* Decide at most once.
          /\ accepted' = CASE Mode = "noIdGuard" -> idMatches /\ siblingsMatch  \* The broken rule skips the ID guard.
                       [] Mode = "ignoreSibling" -> hasId /\ idMatches  \* This broken rule skips a sibling.
                       [] OTHER -> hasId /\ idMatches /\ siblingsMatch  \* The correct rule requires all three.
          /\ decided' = TRUE  \* Record the decision.
          /\ UNCHANGED <<hasId, idMatches, siblingsMatch>>  \* Inputs cannot change during evaluation.
Next == Decide  \* Evaluation is the only action.
Spec == Init /\ [][Next]_vars  \* Begin at Init and evaluate or stutter.
TypeOK == /\ hasId \in BOOLEAN /\ idMatches \in BOOLEAN /\ siblingsMatch \in BOOLEAN  \* Inputs remain Boolean.
          /\ decided \in BOOLEAN /\ accepted \in BOOLEAN  \* So does the decision.
Safe == accepted => (hasId /\ idMatches /\ siblingsMatch)  \* Acceptance requires every condition.
Exact == decided => (accepted = (hasId /\ idMatches /\ siblingsMatch))  \* The decision matches the conjunction.
====
