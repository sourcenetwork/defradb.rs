---- MODULE MissingRelationFilter ----
\* A missing child has null fields. Scalar evaluation supplies idMatches and
\* siblingsMatch; this model checks the join's admission rule, not the scalar
\* evaluator or the document mapping.
EXTENDS Naturals
CONSTANTS HasId, IdMatches, SiblingsMatch, Mode
ASSUME /\ HasId \in BOOLEAN /\ IdMatches \in BOOLEAN
       /\ SiblingsMatch \in BOOLEAN
       /\ Mode \in {"correct", "noIdGuard", "ignoreSibling"}
VARIABLES decided, accepted
vars == <<decided, accepted>>
Init == /\ decided = FALSE /\ accepted = FALSE
Decide ==
  /\ ~decided
  /\ accepted' = CASE Mode = "noIdGuard" -> IdMatches /\ SiblingsMatch
                  [] Mode = "ignoreSibling" -> HasId /\ IdMatches
                  [] OTHER -> HasId /\ IdMatches /\ SiblingsMatch
  /\ decided' = TRUE
Next == Decide
Spec == Init /\ [][Next]_vars
TypeOK == /\ decided \in BOOLEAN /\ accepted \in BOOLEAN
Safe == accepted => (HasId /\ IdMatches /\ SiblingsMatch)
Exact == decided => (accepted = (HasId /\ IdMatches /\ SiblingsMatch))
====
